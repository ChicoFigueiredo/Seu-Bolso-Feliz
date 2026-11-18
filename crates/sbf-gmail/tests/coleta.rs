//! A coleta do Gmail com um cliente de mentira e dados fictícios: nenhuma rede, nenhuma
//! credencial, nenhum e-mail de verdade.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rusqlite::Connection;
use sbf_coletor::gmail::cliente::{PaginaDeMensagens, ReferenciaDeMensagem};
use sbf_coletor::gmail::mensagem::MensagemDoGmail;
use sbf_gmail::{
    Desfecho, Erro, FonteDeGmail, OpcoesDaColeta, Reconhecimento, ReconhecimentoDoCofre, Resultado,
    SituacaoDaConexao, coletar, estado_do_gmail,
};
use sbf_inventario::banco::Inventario;
use sbf_pdf::{ErroDeFonte, FonteDeSegredos};
use sbf_reconhecimento::ReconhecedorDoCofre;
use sbf_reconhecimento::regras::MensagemRecebida;
use sbf_regras::catalogo::{Catalogo, NovaVersao};
use sbf_regras::definicao::{Condicao, Definicao};
use sbf_regras::{Autor, Canal, Quem, TipoDeEntrada};
use serde_json::{Value, json};
use zeroize::Zeroizing;

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

const PDF: &[u8] = b"%PDF-1.4 ficticio: boleto de teste";
const NFE: &str = "<nfeProc><NFe><infNFe><emit><xNome>Loja Exemplo Ltda</xNome></emit>\
<total><vNF>187.50</vNF></total></infNFe></NFe></nfeProc>";

/// Uma mensagem com os anexos `(parte, nome, mime, conteudo, embutido)` e o corpo dado.
struct Msg {
    id: &'static str,
    ms: i64,
    assunto: &'static str,
    corpo: &'static str,
    anexos: Vec<(&'static str, &'static str, &'static str, Vec<u8>, bool)>,
}

fn msg(id: &'static str, ms: i64, assunto: &'static str) -> Msg {
    Msg {
        id,
        ms,
        assunto,
        corpo: "",
        anexos: Vec::new(),
    }
}

impl Msg {
    fn corpo(mut self, c: &'static str) -> Self {
        self.corpo = c;
        self
    }
    fn anexo(
        mut self,
        parte: &'static str,
        nome: &'static str,
        mime: &'static str,
        c: &[u8],
    ) -> Self {
        self.anexos.push((parte, nome, mime, c.to_vec(), false));
        self
    }
    fn embutido(mut self, parte: &'static str, nome: &'static str, c: &[u8]) -> Self {
        self.anexos
            .push((parte, nome, "image/png", c.to_vec(), true));
        self
    }
    fn json(&self) -> Value {
        let mut partes = vec![json!({
            "partId": "0", "mimeType": "text/plain", "filename": "",
            "body": {"size": self.corpo.len(), "data": b64(self.corpo.as_bytes())}
        })];
        for (parte, nome, mime, c, embutido) in &self.anexos {
            let cabecalhos = if *embutido {
                json!([{"name": "Content-ID", "value": "<logo@exemplo>"}])
            } else {
                json!([])
            };
            partes.push(json!({
                "partId": parte, "mimeType": mime, "filename": nome, "headers": cabecalhos,
                "body": {"attachmentId": format!("anexo-{}-{parte}", self.id), "size": c.len()}
            }));
        }
        json!({
            "id": self.id, "threadId": format!("t-{}", self.id), "labelIds": [],
            "internalDate": self.ms.to_string(),
            "payload": {
                "mimeType": "multipart/mixed",
                "headers": [
                    {"name": "Subject", "value": self.assunto},
                    {"name": "From", "value": "Cobranca <cobranca@exemplo.test>"},
                    {"name": "To", "value": "dono@exemplo.test"}
                ],
                "body": {"size": 0},
                "parts": partes
            }
        })
    }
}

#[derive(Default)]
struct Falsa {
    mensagens: Vec<Msg>,
    consultas: RefCell<Vec<String>>,
    anexos_baixados: Cell<usize>,
    mensagens_lidas: Cell<usize>,
}

impl Falsa {
    fn com(mensagens: Vec<Msg>) -> Self {
        Self {
            mensagens,
            ..Self::default()
        }
    }
}

impl FonteDeGmail for Falsa {
    fn listar(&self, consulta: &str, _pagina: Option<&str>) -> Resultado<PaginaDeMensagens> {
        self.consultas.borrow_mut().push(consulta.to_owned());
        // O Gmail devolve do mais novo para o mais velho.
        // `after:<segundos>`, como o Gmail: só as mensagens depois do instante.
        let depois: i64 = consulta
            .split_whitespace()
            .find_map(|t| t.strip_prefix("after:"))
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let mut ordenadas: Vec<&Msg> = self
            .mensagens
            .iter()
            .filter(|m| m.ms / 1000 > depois)
            .collect();
        ordenadas.sort_by_key(|m| std::cmp::Reverse(m.ms));
        Ok(PaginaDeMensagens {
            messages: ordenadas
                .into_iter()
                .map(|m| ReferenciaDeMensagem {
                    id: m.id.to_owned(),
                })
                .collect(),
            next_page_token: None,
        })
    }

    fn mensagem(&self, id: &str) -> Resultado<MensagemDoGmail> {
        self.mensagens_lidas.set(self.mensagens_lidas.get() + 1);
        let m = self
            .mensagens
            .iter()
            .find(|m| m.id == id)
            .ok_or_else(|| Erro::Gmail("não existe".to_owned()))?;
        Ok(serde_json::from_value(m.json()).expect("mensagem"))
    }

    fn anexo(&self, id_da_mensagem: &str, id_do_anexo: &str) -> Resultado<Vec<u8>> {
        self.anexos_baixados.set(self.anexos_baixados.get() + 1);
        let m = self
            .mensagens
            .iter()
            .find(|m| m.id == id_da_mensagem)
            .ok_or_else(|| Erro::Gmail("não existe".to_owned()))?;
        m.anexos
            .iter()
            .find(|a| format!("anexo-{}-{}", m.id, a.0) == id_do_anexo)
            .map(|a| a.3.clone())
            .ok_or_else(|| Erro::Gmail("anexo não existe".to_owned()))
    }
}

/// Registra o que o reconhecimento recebeu; `falhar_em` faz falhar o arquivo com esse nome.
#[derive(Default)]
struct Registrador {
    arquivos: RefCell<Vec<(String, String)>>,
    mensagens: RefCell<Vec<(String, String)>>,
    falhar_em: Option<&'static str>,
}

impl Reconhecimento for Registrador {
    fn arquivo(&self, caminho: &Path, id_externo: &str) -> Result<Desfecho, String> {
        let nome = caminho.file_name().unwrap().to_string_lossy().into_owned();
        if self.falhar_em.is_some_and(|f| nome.ends_with(f)) {
            return Err("falha de mentira".to_owned());
        }
        assert!(caminho.is_file(), "o arquivo já está no cofre");
        self.arquivos
            .borrow_mut()
            .push((nome, id_externo.to_owned()));
        Ok(Desfecho::Nada)
    }

    fn mensagem(&self, m: &MensagemRecebida<'_>) -> Result<Desfecho, String> {
        assert_eq!(m.canal, Canal::Gmail);
        self.mensagens
            .borrow_mut()
            .push((m.id_externo.to_owned(), m.texto.to_owned()));
        Ok(Desfecho::Reconhecido("aviso".to_owned()))
    }
}

struct Cenario {
    _raiz: tempfile::TempDir,
    cofre: PathBuf,
}

fn cenario() -> Cenario {
    let raiz = tempfile::tempdir().expect("raiz");
    let cofre = raiz.path().join("cofre");
    std::fs::create_dir_all(&cofre).expect("cofre");
    let mut inventario = Inventario::abrir(cofre.join("inventario.sqlite")).expect("abrir");
    inventario
        .migrar(raiz.path().join("copia.sqlite"))
        .expect("migrar");
    Cenario { _raiz: raiz, cofre }
}

impl Cenario {
    fn conn(&self) -> Connection {
        Connection::open(self.cofre.join("inventario.sqlite")).expect("abrir")
    }
    fn contar(&self, sql: &str) -> i64 {
        self.conn()
            .query_row(sql, [], |l| l.get(0))
            .expect("contar")
    }
    fn coletar(
        &self,
        f: &Falsa,
        r: &Registrador,
        o: &OpcoesDaColeta,
    ) -> Resultado<sbf_gmail::ResumoDaColeta> {
        coletar(&self.cofre, f, r, o, &AtomicBool::new(false))
    }
}

const T0: i64 = 1_794_000_000_000;

#[test]
fn anexo_novo_vai_para_entrada_gmail_com_origem_e_passa_pelo_reconhecimento() {
    let c = cenario();
    let f = Falsa::com(vec![msg("m1", T0, "Boleto de novembro").anexo(
        "1",
        "Boleto Novembro.pdf",
        "application/pdf",
        PDF,
    )]);
    let r = Registrador::default();

    let resumo = c.coletar(&f, &r, &OpcoesDaColeta::default()).unwrap();

    assert_eq!(resumo.mensagens, 1);
    assert_eq!(resumo.anexos_novos, 1);
    assert_eq!(resumo.documentos_novos, 1);
    // O arquivo está em entrada/gmail/ e o inventário o conhece, com a origem do Gmail.
    let pasta = c.cofre.join("entrada/gmail");
    let arquivos: Vec<_> = std::fs::read_dir(&pasta).unwrap().collect();
    assert_eq!(arquivos.len(), 1);
    assert_eq!(
        std::fs::read(arquivos[0].as_ref().unwrap().path()).unwrap(),
        PDF
    );
    assert_eq!(c.contar("SELECT count(*) FROM achado"), 1);
    let (chave, tipo, msg_id, assunto, papel): (String, String, String, String, String) = c
        .conn()
        .query_row(
            "SELECT origin_key, origin_type, gmail_message_id, gmail_subject, mensagem_ref FROM origem",
            [],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?, l.get(4)?)),
        )
        .unwrap();
    assert_eq!(chave, "gmail:m1:1");
    assert_eq!(tipo, "gmail");
    assert_eq!(msg_id, "m1");
    assert_eq!(assunto, "Boleto de novembro");
    assert_eq!(papel, "anexo");
    let caminho: String = c
        .conn()
        .query_row("SELECT caminho_acervo FROM achado", [], |l| l.get(0))
        .unwrap();
    assert!(caminho.starts_with("entrada/gmail/"));
    assert!(c.cofre.join(&caminho).is_file());
    // O reconhecimento recebeu o arquivo do cofre, com o id da mensagem.
    let vistos = r.arquivos.borrow();
    assert_eq!(vistos.len(), 1);
    assert_eq!(vistos[0].1, "m1");
}

#[test]
fn o_mesmo_pdf_em_dois_emails_e_um_documento_com_duas_origens() {
    let c = cenario();
    let f = Falsa::com(vec![
        msg("m1", T0, "Boleto").anexo("1", "boleto.pdf", "application/pdf", PDF),
        msg("m2", T0 + 60_000, "Reenvio do boleto").anexo(
            "1",
            "segunda-via.pdf",
            "application/pdf",
            PDF,
        ),
    ]);
    let r = Registrador::default();

    let resumo = c.coletar(&f, &r, &OpcoesDaColeta::default()).unwrap();

    assert_eq!(resumo.anexos_novos, 2);
    assert_eq!(resumo.documentos_novos, 1);
    assert_eq!(resumo.duplicados, 1);
    assert_eq!(c.contar("SELECT count(*) FROM achado"), 1);
    assert_eq!(c.contar("SELECT count(*) FROM origem"), 2);
    assert_eq!(
        std::fs::read_dir(c.cofre.join("entrada/gmail"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(r.arquivos.borrow().len(), 1, "reconhecido uma vez só");
}

#[test]
fn coletar_de_novo_nao_baixa_nem_reconhece_nada() {
    let c = cenario();
    let f = Falsa::com(vec![
        msg("m1", T0, "Boleto").anexo("1", "boleto.pdf", "application/pdf", PDF),
        msg("m2", T0 + 1000, "Aviso").corpo("Seu boleto vence em 24/11."),
    ]);
    let r = Registrador::default();
    c.coletar(&f, &r, &OpcoesDaColeta::default()).unwrap();
    let baixados = f.anexos_baixados.get();

    let resumo = c.coletar(&f, &r, &OpcoesDaColeta::default()).unwrap();

    assert_eq!(
        f.anexos_baixados.get(),
        baixados,
        "origem conhecida não baixa de novo"
    );
    assert_eq!(resumo.ja_conhecidos, 2);
    assert_eq!(resumo.documentos_novos, 0);
    assert_eq!(r.arquivos.borrow().len(), 1);
    assert_eq!(r.mensagens.borrow().len(), 1);
    assert_eq!(c.contar("SELECT count(*) FROM origem"), 2);
}

#[test]
fn corpo_sem_anexo_vira_mensagem_com_o_texto_e_passa_pelas_regras() {
    let c = cenario();
    let f = Falsa::com(vec![
        msg("m7", T0, "Aviso de boleto").corpo("Seu boleto vence em 24/11/2026."),
    ]);
    let r = Registrador::default();

    let resumo = c.coletar(&f, &r, &OpcoesDaColeta::default()).unwrap();

    assert_eq!(resumo.mensagens_de_texto, 1);
    assert_eq!(resumo.reconhecidos, 1);
    let vistas = r.mensagens.borrow();
    assert_eq!(vistas.len(), 1);
    assert_eq!(vistas[0].0, "m7");
    assert_eq!(
        vistas[0].1,
        "Aviso de boleto\n\nSeu boleto vence em 24/11/2026."
    );
    assert!(r.arquivos.borrow().is_empty(), "o corpo não é arquivo");
    let (chave, papel): (String, String) = c
        .conn()
        .query_row("SELECT origin_key, mensagem_ref FROM origem", [], |l| {
            Ok((l.get(0)?, l.get(1)?))
        })
        .unwrap();
    assert_eq!(chave, "gmail:m7:corpo");
    assert_eq!(papel, "eml");
    assert_eq!(
        c.contar("SELECT count(*) FROM achado WHERE mime_type = 'text/plain'"),
        1
    );
}

#[test]
fn imagem_embutida_na_assinatura_nao_e_documento() {
    let c = cenario();
    let f = Falsa::com(vec![
        msg("m1", T0, "Fatura")
            .anexo("1", "fatura.pdf", "application/pdf", PDF)
            .embutido("2", "image001.png", b"png de assinatura"),
    ]);
    let r = Registrador::default();

    let resumo = c.coletar(&f, &r, &OpcoesDaColeta::default()).unwrap();

    assert_eq!(resumo.anexos_novos, 1);
    assert_eq!(f.anexos_baixados.get(), 1);
}

#[test]
fn a_consulta_padrao_e_o_checkpoint_entram_na_listagem() {
    let c = cenario();
    let f = Falsa::com(vec![msg("m1", T0, "Boleto").anexo(
        "1",
        "boleto.pdf",
        "application/pdf",
        PDF,
    )]);
    let r = Registrador::default();

    c.coletar(&f, &r, &OpcoesDaColeta::default()).unwrap();
    c.coletar(&f, &r, &OpcoesDaColeta::default()).unwrap();

    let q = f.consultas.borrow();
    assert_eq!(q[0], "has:attachment newer_than:30d -category:promotions");
    // O checkpoint é o internalDate da última mensagem, com uma hora de folga.
    assert_eq!(
        q[1],
        format!(
            "has:attachment newer_than:30d -category:promotions after:{}",
            T0 / 1000 - 3600
        )
    );
}

#[test]
fn desde_e_consulta_do_usuario_ignoram_o_checkpoint() {
    let c = cenario();
    let f = Falsa::com(vec![msg("m1", T0, "Boleto").anexo(
        "1",
        "boleto.pdf",
        "application/pdf",
        PDF,
    )]);
    let r = Registrador::default();
    c.coletar(&f, &r, &OpcoesDaColeta::default()).unwrap();

    let o = OpcoesDaColeta {
        consulta: Some("from:cobranca@exemplo.test".to_owned()),
        desde: Some("2026-10-01".to_owned()),
        limite: None,
    };
    c.coletar(&f, &r, &o).unwrap();

    assert_eq!(
        f.consultas.borrow()[1],
        "from:cobranca@exemplo.test after:2026/10/01"
    );
    let ruim = OpcoesDaColeta {
        desde: Some("ontem".to_owned()),
        ..OpcoesDaColeta::default()
    };
    assert!(matches!(c.coletar(&f, &r, &ruim), Err(Erro::Argumento(_))));
}

#[test]
fn falha_numa_mensagem_nao_impede_as_outras_e_congela_o_checkpoint() {
    let c = cenario();
    let f = Falsa::com(vec![
        msg("m1", T0, "Primeiro").anexo("1", "primeiro.pdf", "application/pdf", PDF),
        msg("m2", T0 + 1000, "Segundo").anexo(
            "1",
            "segundo.pdf",
            "application/pdf",
            b"%PDF segundo",
        ),
        msg("m3", T0 + 2000, "Terceiro").anexo(
            "1",
            "terceiro.pdf",
            "application/pdf",
            b"%PDF terceiro",
        ),
    ]);
    let r = Registrador {
        falhar_em: Some("segundo.pdf"),
        ..Registrador::default()
    };

    let resumo = c.coletar(&f, &r, &OpcoesDaColeta::default()).unwrap();

    assert_eq!(resumo.falhas, 1);
    assert_eq!(resumo.documentos_novos, 3, "as três foram guardadas");
    assert_eq!(
        r.arquivos.borrow().len(),
        2,
        "o reconhecimento da segunda falhou"
    );
    // O checkpoint parou na primeira, antes da que falhou.
    c.coletar(&f, &Registrador::default(), &OpcoesDaColeta::default())
        .unwrap();
    assert!(
        f.consultas.borrow()[1].ends_with(&format!("after:{}", T0 / 1000 - 3600)),
        "{:?}",
        f.consultas.borrow()
    );
}

#[test]
fn o_limite_deixa_as_mais_novas_para_a_proxima_coleta() {
    let c = cenario();
    let f = Falsa::com(vec![
        msg("m1", T0, "A").anexo("1", "a.pdf", "application/pdf", b"%PDF a"),
        msg("m2", T0 + 1000, "B").anexo("1", "b.pdf", "application/pdf", b"%PDF b"),
        msg("m3", T0 + 2000, "C").anexo("1", "c.pdf", "application/pdf", b"%PDF c"),
    ]);
    let r = Registrador::default();
    let o = OpcoesDaColeta {
        limite: Some(2),
        ..OpcoesDaColeta::default()
    };

    let resumo = c.coletar(&f, &r, &o).unwrap();

    assert_eq!(resumo.mensagens, 2);
    assert_eq!(resumo.restantes, 1);
    // As duas mais velhas entraram; a próxima coleta pega a que sobrou.
    let resumo = c.coletar(&f, &r, &o).unwrap();
    assert_eq!(resumo.documentos_novos, 1);
    assert_eq!(c.contar("SELECT count(*) FROM achado"), 3);
}

#[test]
fn estado_da_conexao_conta_o_que_o_inventario_sabe() {
    let c = cenario();
    let antes = estado_do_gmail(&c.cofre);
    assert_eq!(antes.origens, 0);
    assert!(antes.ultima_coleta.is_none());

    let f = Falsa::com(vec![msg("m1", T0, "Boleto").anexo(
        "1",
        "boleto.pdf",
        "application/pdf",
        PDF,
    )]);
    c.coletar(&f, &Registrador::default(), &OpcoesDaColeta::default())
        .unwrap();

    let depois = estado_do_gmail(&c.cofre);
    assert_eq!(depois.origens, 1);
    assert_eq!(depois.documentos, 1);
    assert!(depois.ultima_coleta.is_some());
    assert!(depois.ultima_falha.is_none());
    if depois.faltando.is_empty() {
        assert_eq!(depois.situacao, SituacaoDaConexao::Coletada);
    } else {
        assert_eq!(depois.situacao, SituacaoDaConexao::SemCredenciais);
    }
}

// ---------------------------------------------------------------- com o reconhecedor de verdade

struct SemSegredos;

impl FonteDeSegredos for SemSegredos {
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
        Err(ErroDeFonte::Indisponivel)
    }
    fn senha_propria(&self, _: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
        Err(ErroDeFonte::Indisponivel)
    }
}

fn instalar_regra(cofre: &Path, regra: &str, tipo: TipoDeEntrada, contem: &str) {
    let mut d = Definicao::simples("exemplo", contem);
    d.aplica_a = vec![tipo];
    d.detectar.todos.push(Condicao::Canal(vec![Canal::Gmail]));
    let mut cat = Catalogo::abrir(&cofre.join("regras.sqlite")).expect("catalogo");
    cat.semear_fabrica().expect("fabrica");
    let v = cat
        .salvar_rascunho(NovaVersao {
            regra: regra.to_owned(),
            nome: regra.to_owned(),
            definicao: d,
            script: None,
            autor: Autor::Usuario,
            motivo: "teste".to_owned(),
        })
        .expect("rascunho");
    cat.ativar(&v.regra, v.versao, &Quem::Usuario, None)
        .expect("ativar");
}

#[test]
fn com_o_reconhecedor_real_a_nfe_e_o_aviso_passam_pelas_regras_do_cofre() {
    let c = cenario();
    instalar_regra(&c.cofre, "nfe-exemplo", TipoDeEntrada::Documento, "nfeproc");
    instalar_regra(
        &c.cofre,
        "aviso-de-boleto",
        TipoDeEntrada::Mensagem,
        "boleto vence",
    );
    let f = Falsa::com(vec![
        msg("m1", T0, "Nota fiscal").anexo("1", "nfe.xml", "application/xml", NFE.as_bytes()),
        msg("m2", T0 + 1000, "Aviso").corpo("Banco Exemplo informa: seu boleto vence em 24/11."),
        msg("m3", T0 + 2000, "Bom dia").corpo("Vamos almoçar?"),
    ]);
    let real = ReconhecimentoDoCofre::novo(
        ReconhecedorDoCofre::novo(&c.cofre, Arc::new(SemSegredos), None).com_regras(None),
    );

    let resumo = coletar(
        &c.cofre,
        &f,
        &real,
        &OpcoesDaColeta::default(),
        &AtomicBool::new(false),
    )
    .unwrap();

    assert_eq!(resumo.reconhecidos, 2, "{resumo:?}");
    assert_eq!(resumo.sem_regra, 1);
    assert_eq!(resumo.falhas, 0);
    // O texto do XML foi para a tabela `texto` do inventário.
    assert_eq!(c.contar("SELECT count(*) FROM texto"), 1);
    // As duas regras deixaram rastro de execução.
    let regras = Connection::open(c.cofre.join("regras.sqlite")).unwrap();
    let n: i64 = regras
        .query_row("SELECT count(*) FROM execucao", [], |l| l.get(0))
        .unwrap();
    assert!(n >= 2, "{n} execuções");
}
