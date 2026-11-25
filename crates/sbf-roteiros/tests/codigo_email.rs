//! O código do MFA por e-mail: caixa falsa e Gmail gravado (fixtures fictícias, sem rede).
//! Fica fora do `src/` porque confere o valor do código com `Segredo::expor`, que o guarda de
//! `seguranca_fontes` só admite em arquivos de produção nomeados.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod codigo_email {
    use std::cell::{Cell, RefCell};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use sbf_chaveiro::Segredo;
    use sbf_coletor::gmail::cliente::PaginaDeMensagens;
    use sbf_coletor::gmail::mensagem::MensagemDoGmail;
    use sbf_gmail::Erro as ErroDoGmail;
    use sbf_gmail::Resultado;
    use sbf_gmail::fonte::{FonteDeGmail, Rotulo};
    use sbf_roteiros::codigo_email::{
        BuscaDeCodigo, CaixaDeEmail, CaixaGmail, ErroDeCodigo, JANELA_MAXIMA, MAXIMO_DE_MENSAGENS,
        MensagemCurta, Relogio, esperar_codigo,
    };
    use zeroize::Zeroizing;

    const REMETENTE: &str = "nao-responda@portal.exemplo.test";

    fn t(segundos: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_700_000_000 + segundos)
    }

    fn msg(segundos: u64, texto: &str) -> MensagemCurta {
        MensagemCurta {
            recebida_em: t(segundos),
            corpo: Zeroizing::new(texto.to_owned()),
        }
    }

    /// O relógio que só anda quando se espera.
    struct RelogioFalso {
        agora: Cell<SystemTime>,
        esperas: Cell<usize>,
    }

    impl RelogioFalso {
        fn em(segundos: u64) -> Self {
            Self {
                agora: Cell::new(t(segundos)),
                esperas: Cell::new(0),
            }
        }
    }

    impl Relogio for RelogioFalso {
        fn agora(&self) -> SystemTime {
            self.agora.get()
        }
        fn esperar(&self, duracao: Duration) {
            self.esperas.set(self.esperas.get() + 1);
            self.agora.set(self.agora.get() + duracao);
        }
    }

    /// A caixa falsa: cada consulta devolve a próxima resposta (a última se repete) e só
    /// entrega mensagens recebidas a partir de `desde`, do remetente combinado.
    struct CaixaFalsa {
        remetente: &'static str,
        respostas: RefCell<Vec<Result<Vec<MensagemCurta>, ErroDeCodigo>>>,
        consultas: Cell<usize>,
    }

    impl CaixaFalsa {
        fn com(mensagens: Vec<MensagemCurta>) -> Self {
            Self::sequencia(vec![Ok(mensagens)])
        }
        fn sequencia(r: Vec<Result<Vec<MensagemCurta>, ErroDeCodigo>>) -> Self {
            Self {
                remetente: REMETENTE,
                respostas: RefCell::new(r),
                consultas: Cell::new(0),
            }
        }
    }

    impl CaixaDeEmail for CaixaFalsa {
        fn mensagens_recentes(
            &self,
            remetente: &str,
            desde: SystemTime,
        ) -> Result<Vec<MensagemCurta>, ErroDeCodigo> {
            self.consultas.set(self.consultas.get() + 1);
            if remetente != self.remetente {
                return Ok(Vec::new());
            }
            let mut r = self.respostas.borrow_mut();
            let resposta = if r.len() > 1 {
                r.remove(0)
            } else {
                r[0].clone_resposta()
            };
            resposta.map(|ms| {
                ms.into_iter()
                    .filter(|m| m.recebida_em >= desde)
                    .collect::<Vec<_>>()
            })
        }
    }

    trait ClonarResposta {
        fn clone_resposta(&self) -> Self;
    }
    impl ClonarResposta for Result<Vec<MensagemCurta>, ErroDeCodigo> {
        fn clone_resposta(&self) -> Self {
            match self {
                Ok(v) => Ok(v
                    .iter()
                    .map(|m| MensagemCurta {
                        recebida_em: m.recebida_em,
                        corpo: m.corpo.clone(),
                    })
                    .collect()),
                Err(e) => Err(*e),
            }
        }
    }

    fn busca() -> BuscaDeCodigo {
        BuscaDeCodigo::nova(REMETENTE, r"\d{6}")
            .expect("busca")
            .com_janela(Duration::from_secs(60))
            .com_intervalo(Duration::from_secs(5))
    }

    fn esperar(caixa: &CaixaFalsa, relogio: &RelogioFalso) -> Result<Segredo, ErroDeCodigo> {
        esperar_codigo(caixa, &busca(), t(0), relogio)
    }

    #[test]
    fn devolve_o_codigo_fresco() {
        let caixa = CaixaFalsa::com(vec![msg(
            10,
            "Seu código de acesso: 123456. Não compartilhe.",
        )]);
        let relogio = RelogioFalso::em(0);
        let codigo = esperar(&caixa, &relogio).expect("código");
        assert_eq!(codigo.expor(), "123456");
        assert_eq!(relogio.esperas.get(), 0, "achou na primeira consulta");
    }

    #[test]
    fn ignora_o_codigo_velho_mesmo_que_a_caixa_o_entregue() {
        // Uma caixa descuidada entrega mensagem de antes do pedido: a função não a aceita.
        struct Descuidada(Vec<MensagemCurta>);
        impl CaixaDeEmail for Descuidada {
            fn mensagens_recentes(
                &self,
                _: &str,
                _: SystemTime,
            ) -> Result<Vec<MensagemCurta>, ErroDeCodigo> {
                Ok(self
                    .0
                    .iter()
                    .map(|m| MensagemCurta {
                        recebida_em: m.recebida_em,
                        corpo: m.corpo.clone(),
                    })
                    .collect())
            }
        }
        let caixa = Descuidada(vec![msg(0, "código 111111"), msg(0, "código 222222")]);
        let relogio = RelogioFalso::em(5);
        let r = esperar_codigo(&caixa, &busca(), t(5), &relogio);
        // Nem no instante exato do pedido: tem de ser depois.
        assert_eq!(r.unwrap_err(), ErroDeCodigo::Expirou);
    }

    #[test]
    fn codigo_velho_nao_vale_e_o_novo_que_chega_depois_vale() {
        let caixa = CaixaFalsa::sequencia(vec![
            Ok(vec![msg(1, "código velho 111111")]),
            Ok(vec![
                msg(1, "código velho 111111"),
                msg(20, "código novo 654321"),
            ]),
        ]);
        let relogio = RelogioFalso::em(10);
        let codigo = esperar_codigo(&caixa, &busca(), t(10), &relogio).expect("código");
        assert_eq!(codigo.expor(), "654321");
        assert_eq!(caixa.consultas.get(), 2);
        assert_eq!(relogio.esperas.get(), 1);
    }

    #[test]
    fn com_duas_mensagens_pega_a_mais_nova() {
        let caixa = CaixaFalsa::com(vec![
            msg(30, "código 222222"),
            msg(10, "código 111111"),
            msg(20, "sem número nesta"),
        ]);
        let relogio = RelogioFalso::em(0);
        assert_eq!(esperar(&caixa, &relogio).expect("código").expor(), "222222");
    }

    #[test]
    fn dois_codigos_diferentes_na_mesma_mensagem_sao_ambiguos() {
        let caixa = CaixaFalsa::com(vec![msg(10, "use 123456 ou então 654321")]);
        let relogio = RelogioFalso::em(0);
        assert_eq!(
            esperar(&caixa, &relogio).unwrap_err(),
            ErroDeCodigo::Ambiguo
        );
    }

    #[test]
    fn o_mesmo_codigo_repetido_na_mensagem_nao_e_ambiguo() {
        let caixa = CaixaFalsa::com(vec![msg(10, "Código 123456\nrepetindo: 123456")]);
        let relogio = RelogioFalso::em(0);
        assert_eq!(esperar(&caixa, &relogio).expect("código").expor(), "123456");
    }

    #[test]
    fn duas_mensagens_no_mesmo_instante_com_codigos_diferentes_sao_ambiguas() {
        let caixa = CaixaFalsa::com(vec![msg(10, "código 111111"), msg(10, "código 222222")]);
        let relogio = RelogioFalso::em(0);
        assert_eq!(
            esperar(&caixa, &relogio).unwrap_err(),
            ErroDeCodigo::Ambiguo
        );
    }

    #[test]
    fn remetente_errado_nao_entrega_nada_e_expira() {
        let caixa = CaixaFalsa::com(vec![msg(10, "código 123456")]);
        let busca = BuscaDeCodigo::nova("outro@exemplo.test", r"\d{6}")
            .expect("busca")
            .com_janela(Duration::from_secs(20));
        let relogio = RelogioFalso::em(0);
        let r = esperar_codigo(&caixa, &busca, t(0), &relogio);
        assert_eq!(r.unwrap_err(), ErroDeCodigo::Expirou);
    }

    #[test]
    fn formato_que_nao_casa_e_sem_codigo() {
        let caixa = CaixaFalsa::com(vec![msg(
            10,
            "seu código é 12345 (cinco) ou 1234567 (sete)",
        )]);
        let relogio = RelogioFalso::em(0);
        assert_eq!(
            esperar(&caixa, &relogio).unwrap_err(),
            ErroDeCodigo::SemCodigo
        );
    }

    #[test]
    fn expira_sem_dormir_de_verdade_e_respeita_a_janela() {
        let caixa = CaixaFalsa::com(Vec::new());
        let relogio = RelogioFalso::em(0);
        let inicio = relogio.agora();
        assert_eq!(
            esperar(&caixa, &relogio).unwrap_err(),
            ErroDeCodigo::Expirou
        );
        assert_eq!(
            relogio.agora().duration_since(inicio).expect("tempo"),
            Duration::from_secs(60)
        );
        assert_eq!(relogio.esperas.get(), 12);
    }

    #[test]
    fn a_janela_e_cortada_em_300_segundos() {
        let b = busca().com_janela(Duration::from_secs(9999));
        assert_eq!(b.janela, JANELA_MAXIMA);
        let caixa = CaixaFalsa::com(Vec::new());
        let relogio = RelogioFalso::em(0);
        let inicio = relogio.agora();
        let r = esperar_codigo(&caixa, &b, t(0), &relogio);
        assert_eq!(r.unwrap_err(), ErroDeCodigo::Expirou);
        assert_eq!(
            relogio.agora().duration_since(inicio).expect("tempo"),
            JANELA_MAXIMA
        );
    }

    #[test]
    fn caixa_indisponivel_tenta_ate_o_prazo_e_se_recupera() {
        let permanente = CaixaFalsa::sequencia(vec![Err(ErroDeCodigo::CaixaIndisponivel)]);
        let relogio = RelogioFalso::em(0);
        assert_eq!(
            esperar(&permanente, &relogio).unwrap_err(),
            ErroDeCodigo::CaixaIndisponivel
        );
        let passageira = CaixaFalsa::sequencia(vec![
            Err(ErroDeCodigo::CaixaIndisponivel),
            Ok(vec![msg(10, "código 123456")]),
        ]);
        let relogio = RelogioFalso::em(0);
        assert_eq!(
            esperar(&passageira, &relogio).expect("código").expor(),
            "123456"
        );
    }

    #[test]
    fn autorizacao_para_na_hora() {
        let caixa = CaixaFalsa::sequencia(vec![Err(ErroDeCodigo::Autorizacao)]);
        let relogio = RelogioFalso::em(0);
        assert_eq!(
            esperar(&caixa, &relogio).unwrap_err(),
            ErroDeCodigo::Autorizacao
        );
        assert_eq!(caixa.consultas.get(), 1);
        assert_eq!(relogio.esperas.get(), 0);
    }

    #[test]
    fn erro_e_debug_nao_carregam_o_codigo_nem_o_texto() {
        let caixa = CaixaFalsa::com(vec![msg(10, "Assunto-sigiloso 123456 e 654321")]);
        let relogio = RelogioFalso::em(0);
        let erro = esperar(&caixa, &relogio).unwrap_err();
        for texto in [format!("{erro:?}"), erro.to_string()] {
            assert!(
                !texto.contains("123456") && !texto.contains("654321"),
                "{texto}"
            );
            assert!(
                !texto.contains("sigiloso") && !texto.contains("exemplo.test"),
                "{texto}"
            );
        }
        for e in [
            ErroDeCodigo::Expirou,
            ErroDeCodigo::SemCodigo,
            ErroDeCodigo::Ambiguo,
            ErroDeCodigo::CaixaIndisponivel,
            ErroDeCodigo::Autorizacao,
            ErroDeCodigo::Invalido,
        ] {
            assert!(!format!("{e:?} {e}").contains(char::is_numeric));
        }
        // A mensagem e a busca também não se denunciam no Debug.
        let m = msg(1, "código 123456");
        assert!(!format!("{m:?}").contains("123456"));
        assert!(!format!("{:?}", busca()).contains("exemplo.test"));
        // E o segredo devolvido só aparece mascarado.
        let ok = esperar(&CaixaFalsa::com(vec![msg(10, "código 123456")]), &relogio).expect("ok");
        assert!(!format!("{ok:?}").contains("123456"));
    }

    #[test]
    fn busca_recusa_remetente_e_formato_ruins() {
        for r in [
            "",
            "a b@exemplo.test",
            "x OR in:anywhere",
            "a\"b@exemplo.test",
            "a@b@c",
            "{a}@b.test",
        ] {
            assert_eq!(
                BuscaDeCodigo::nova(r, r"\d{6}").unwrap_err(),
                ErroDeCodigo::Invalido,
                "{r}"
            );
        }
        for f in ["", "(", r"\d*", "1)|(2", &"a".repeat(65)] {
            assert_eq!(
                BuscaDeCodigo::nova(REMETENTE, f).unwrap_err(),
                ErroDeCodigo::Invalido,
                "{f}"
            );
        }
        // Domínio sozinho serve de remetente.
        assert!(BuscaDeCodigo::nova("portal.exemplo.test", r"\d{6}").is_ok());
    }

    // --- O adaptador do Gmail, só com respostas gravadas (sem rede) -------------------------

    fn b64(t: &str) -> String {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(t.as_bytes())
    }

    fn gravada(id: &str, de: &str, assunto: &str, corpo: &str, ms: u64) -> MensagemDoGmail {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "internalDate": ms.to_string(),
            "payload": {
                "mimeType": "multipart/mixed",
                "headers": [
                    {"name": "From", "value": de},
                    {"name": "Subject", "value": assunto}
                ],
                "body": {"size": 0},
                "parts": [
                    {"mimeType": "text/plain", "body": {"size": 5, "data": b64(corpo)}},
                    {"mimeType": "application/pdf", "filename": "extrato.pdf",
                     "body": {"attachmentId": "anexo-1", "size": 9}}
                ]
            }
        }))
        .expect("mensagem gravada")
    }

    struct GmailGravado {
        mensagens: Vec<MensagemDoGmail>,
        consultas: RefCell<Vec<String>>,
        anexos_baixados: Cell<usize>,
        falha: Option<fn() -> ErroDoGmail>,
    }

    impl GmailGravado {
        fn com(mensagens: Vec<MensagemDoGmail>) -> Self {
            Self {
                mensagens,
                consultas: RefCell::new(Vec::new()),
                anexos_baixados: Cell::new(0),
                falha: None,
            }
        }
    }

    impl FonteDeGmail for GmailGravado {
        fn listar(
            &self,
            rotulo: Option<&str>,
            consulta: &str,
            pagina: Option<&str>,
        ) -> Resultado<PaginaDeMensagens> {
            assert!(
                rotulo.is_none() && pagina.is_none(),
                "consulta estreita, sem rótulo nem paginação"
            );
            self.consultas.borrow_mut().push(consulta.to_owned());
            if let Some(f) = self.falha {
                return Err(f());
            }
            serde_json::from_value(serde_json::json!({
            "messages": self.mensagens.iter().map(|m| serde_json::json!({"id": m.id})).collect::<Vec<_>>()
        }))
        .map_err(|_| ErroDoGmail::Gmail("fixture".to_owned()))
        }
        fn mensagem(&self, id: &str) -> Resultado<MensagemDoGmail> {
            self.mensagens
                .iter()
                .find(|m| m.id == id)
                .cloned()
                .ok_or_else(|| ErroDoGmail::Gmail("fixture".to_owned()))
        }
        fn anexo(&self, _: &str, _: &str) -> Resultado<Vec<u8>> {
            self.anexos_baixados.set(self.anexos_baixados.get() + 1);
            Ok(Vec::new())
        }
        fn rotulos(&self) -> Resultado<Vec<Rotulo>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn gmail_consulta_estreita_e_extrai_assunto_e_corpo_sem_baixar_anexo() {
        let fonte = GmailGravado::com(vec![gravada(
            "m1",
            "Portal Exemplo <nao-responda@portal.exemplo.test>",
            "Seu código de verificação",
            "Use o código 482913 para entrar.",
            1_700_000_030_000,
        )]);
        let caixa = CaixaGmail::com_fonte(fonte);
        let mensagens = caixa
            .mensagens_recentes(REMETENTE, t(0))
            .expect("mensagens");
        assert_eq!(mensagens.len(), 1);
        assert_eq!(mensagens[0].recebida_em, t(30));
        assert!(
            mensagens[0]
                .corpo
                .starts_with("Seu código de verificação\n")
        );
        assert!(mensagens[0].corpo.contains("482913"));
        assert_eq!(
            caixa.fonte().consultas.borrow().as_slice(),
            ["from:nao-responda@portal.exemplo.test after:1700000000"]
        );
        assert_eq!(
            caixa.fonte().anexos_baixados.get(),
            0,
            "nenhum anexo é baixado"
        );
    }

    #[test]
    fn gmail_descarta_remetente_diferente_e_data_ilegivel_e_limita_a_quantidade() {
        let mut ms = vec![
            gravada(
                "falso",
                "x <nao-responda@portal.exemplo.test.mal.test>",
                "a",
                "999999",
                1_700_000_040_000,
            ),
            gravada(
                "outro",
                "nao-responda@portal.exemplo.test.outro",
                "a",
                "888888",
                1_700_000_040_000,
            ),
        ];
        let mut sem_data = gravada("sem-data", REMETENTE, "a", "777777", 0);
        sem_data.internal_date = "ontem".to_owned();
        ms.push(sem_data);
        for i in 0..8 {
            ms.push(gravada(
                &format!("ok{i}"),
                "NAO-RESPONDA@portal.exemplo.test",
                "a",
                "123456",
                1_700_000_050_000,
            ));
        }
        let caixa = CaixaGmail::com_fonte(GmailGravado::com(ms));
        let r = caixa
            .mensagens_recentes(REMETENTE, t(0))
            .expect("mensagens");
        // Só as 5 primeiras entram; destas, as três primeiras são descartadas por remetente ou data.
        assert_eq!(r.len(), MAXIMO_DE_MENSAGENS - 3);
        assert!(r.iter().all(|m| m.corpo.contains("123456")));
    }

    #[test]
    fn gmail_recusa_remetente_que_alteraria_a_consulta() {
        let caixa = CaixaGmail::com_fonte(GmailGravado::com(Vec::new()));
        let r = caixa.mensagens_recentes("a@b.test OR in:anywhere", t(0));
        assert_eq!(r.unwrap_err(), ErroDeCodigo::Invalido);
        assert!(caixa.fonte().consultas.borrow().is_empty());
    }

    #[test]
    fn gmail_traduz_os_erros_sem_vazar_texto() {
        let mut f = GmailGravado::com(Vec::new());
        f.falha = Some(|| ErroDoGmail::Autorizacao);
        let caixa = CaixaGmail::com_fonte(f);
        assert_eq!(
            caixa.mensagens_recentes(REMETENTE, t(0)).unwrap_err(),
            ErroDeCodigo::Autorizacao
        );
        let mut f = GmailGravado::com(Vec::new());
        f.falha = Some(|| ErroDoGmail::Gmail("o Gmail respondeu HTTP 500".to_owned()));
        let caixa = CaixaGmail::com_fonte(f);
        assert_eq!(
            caixa.mensagens_recentes(REMETENTE, t(0)).unwrap_err(),
            ErroDeCodigo::CaixaIndisponivel
        );
        let mut f = GmailGravado::com(Vec::new());
        f.falha = Some(|| ErroDoGmail::SemCredenciais("GMAIL_REFRESH_TOKEN".to_owned()));
        let caixa = CaixaGmail::com_fonte(f);
        assert_eq!(
            caixa.mensagens_recentes(REMETENTE, t(0)).unwrap_err(),
            ErroDeCodigo::Autorizacao
        );
    }

    #[test]
    fn de_ponta_a_ponta_com_o_gmail_gravado() {
        let fonte = GmailGravado::com(vec![
            gravada(
                "velho",
                REMETENTE,
                "Código",
                "Seu código: 111111",
                1_700_000_005_000,
            ),
            gravada(
                "novo",
                REMETENTE,
                "Código",
                "Seu código: 222222",
                1_700_000_040_000,
            ),
        ]);
        let caixa = CaixaGmail::com_fonte(fonte);
        let relogio = RelogioFalso::em(20);
        let codigo = esperar_codigo(&caixa, &busca(), t(20), &relogio).expect("código");
        assert_eq!(codigo.expor(), "222222");
    }
}
