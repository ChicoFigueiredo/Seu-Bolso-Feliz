//! O motor de regras ligado ao reconhecimento (spec 13, ADR-021): AC-LUA-06, 09, 10, 11 e 14.
//! Cofre temporário, dados fictícios ("Banco Exemplo", "Fulano de Tal"), nenhuma IA.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_inventario::banco::Inventario;
use sbf_pdf::{ErroDeFonte, FonteDeSegredos};
use sbf_planejamento::{
    Alteracao, AlvoDoDocumento, Autor, Banco, Centavos, Certeza, Competencia, Filtro, Fonte,
    Frequencia, Natureza, NovoRecorrente, Periodo, Planejamento, PlanejamentoSqlite, Prazo,
    SemSaldos, Versao,
};
use sbf_reconhecimento::regras::{
    self, ARQUIVO_DAS_REGRAS, entrada_do_documento, entrada_do_registro, ler_registro,
    rotulo_do_candidato,
};
use sbf_reconhecimento::{Reconhecedor, ReconhecedorDoCofre, Reconhecido, Resultado, Sugestao};
use sbf_regras::catalogo::{Catalogo, EstadoDaProposta, NovaVersao};
use sbf_regras::definicao::{
    Condicao, Definicao, Extracao, FonteDaExtracao, Formato, ModeloDeProposta,
};
use sbf_regras::motor::Motor;
use sbf_regras::{Autor as AutorDaRegra, Canal, Decisao, Estado, Quem, Retorno, Valor};
use sbf_script::ExecutorNoProcesso;
use zeroize::Zeroizing;

const CONTRACHEQUE_2026: &str =
    include_str!("../../sbf-planilha/tests/contracheque-2026-ficticio.txt");
const CONTA: &str = "Banco Exemplo 000000-1";
const COMPROVANTE: &str = "Banco Exemplo\nComprovante PIX\nFavorecido: Fulano de Tal\n\
Valor: R$ 187,50\nData: 12/11/2026\nConta de origem: Banco Exemplo 000000-1\n\
Autenticação: ABC123XYZ\n";
const BOLETO_DE_ENERGIA: &str = "BOLETO ENERGIA EXEMPLO S.A.\nVencimento 24/11/2026\nValor R$ 187,50\nConta de origem: Banco Exemplo 000000-1\n";
const FATURA: &str = "Fatura Cartao Exemplo\nCartao: Cartao Exemplo\nTotal da fatura: R$ 900,00\n\
Vencimento: 10/11/2026\n";

struct Segredos;

impl FonteDeSegredos for Segredos {
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
        Ok(Zeroizing::new("52998224725".to_owned()))
    }
    fn senha_propria(&self, _: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
        Err(ErroDeFonte::SemSenhaPropria)
    }
}

struct Cenario {
    _raiz: tempfile::TempDir,
    cofre: PathBuf,
    soltos: PathBuf,
}

fn cenario() -> Cenario {
    let raiz = tempfile::tempdir().expect("raiz");
    let cofre = raiz.path().join("cofre");
    let soltos = raiz.path().join("soltos");
    std::fs::create_dir_all(&cofre).expect("cofre");
    std::fs::create_dir_all(&soltos).expect("soltos");
    let mut inventario = Inventario::abrir(cofre.join("inventario.sqlite")).expect("abrir");
    inventario
        .migrar(raiz.path().join("copia.sqlite"))
        .expect("migrar");
    Cenario {
        _raiz: raiz,
        cofre,
        soltos,
    }
}

fn comp(ano: i32, mes: u8) -> Competencia {
    Competencia { ano, mes }
}

impl Cenario {
    fn conexao(&self) -> Connection {
        Connection::open(self.cofre.join("inventario.sqlite")).expect("abrir")
    }

    fn contar(&self, sql: &str) -> i64 {
        self.conexao()
            .query_row(sql, [], |l| l.get(0))
            .expect("contar")
    }

    fn solto(&self, nome: &str, conteudo: &str) -> PathBuf {
        let c = self.soltos.join(nome);
        std::fs::write(&c, conteudo).expect("escrever");
        c
    }

    fn sem_regras(&self) -> ReconhecedorDoCofre {
        ReconhecedorDoCofre::novo(&self.cofre, Arc::new(Segredos), None)
    }

    fn com_regras(&self) -> ReconhecedorDoCofre {
        self.sem_regras().com_regras(None)
    }

    fn rodar(&self, r: &ReconhecedorDoCofre, caminho: &Path) -> Resultado {
        r.reconhecer(caminho, &mut |_| {}, &AtomicBool::new(false))
            .expect("reconhecer")
    }

    fn plan(&self) -> PlanejamentoSqlite {
        let banco = Banco::abrir(self.cofre.join("planejamento.sqlite")).expect("banco");
        PlanejamentoSqlite::novo(banco, Box::new(SemSaldos))
    }

    /// Um recorrente mensal fictício; devolve o id.
    fn criar(
        &self,
        nome: &str,
        natureza: Natureza,
        valor: i64,
        dia: u8,
        conta: Option<&str>,
    ) -> String {
        let mut plan = self.plan();
        plan.aplicar(
            Alteracao::CriarRecorrente {
                recorrente: NovoRecorrente {
                    nome: nome.into(),
                    grupo: "Casa".into(),
                    subgrupo: None,
                    posicao: 0,
                    natureza,
                    prazo: Prazo::SemFim,
                    primeira_versao: Versao {
                        id: String::new(),
                        desde: comp(2026, 1),
                        valor: Some(Centavos(valor)),
                        certeza: Certeza::Confirmado,
                        fonte: Fonte::Usuario,
                        frequencia: Frequencia::Mensal { dia },
                        conta: conta.map(str::to_owned),
                        centro: None,
                        fornecedor: None,
                        categoria: None,
                        nota: None,
                    },
                },
                origem: None,
            },
            Autor::Usuario,
        )
        .expect("criar");
        plan.recorrentes(false)
            .expect("lista")
            .into_iter()
            .find(|r| r.nome == nome)
            .expect("criado")
            .id
    }

    fn mapear_conta(&self) {
        self.plan()
            .aplicar(
                Alteracao::MapearConta {
                    codigo: "CX-0001".into(),
                    conta_do_extrato: Some(CONTA.into()),
                },
                Autor::Usuario,
            )
            .expect("mapear");
    }

    /// Um movimento no extrato do inventário (a Pluggy).
    fn movimento(&self, id: &str, tipo: &str, data: &str, valor: i64, descricao: &str) {
        let conn = self.conexao();
        conn.execute(
            "INSERT OR IGNORE INTO fonte (id, tipo, descricao)
             VALUES ('conexao:pluggy', 'conexao', 'x')",
            [],
        )
        .expect("fonte");
        conn.execute(
            "INSERT INTO registro (external_ref, fonte_id, tipo, data, data_tipo, valor_centavos,
                                   descricao_original, conta_provavel, extraido_por)
             VALUES (?1, 'conexao:pluggy', ?2, ?3, 'pagamento', ?4, ?5, ?6, 'pluggy@1')",
            rusqlite::params![id, tipo, data, valor, descricao, CONTA],
        )
        .expect("registro");
    }

    fn catalogo(&self) -> Catalogo {
        Catalogo::abrir(&self.cofre.join(ARQUIVO_DAS_REGRAS)).expect("catalogo")
    }

    /// Salva e ativa (pelo usuário) uma regra.
    fn instalar(&self, nova: NovaVersao) {
        let mut c = self.catalogo();
        let v = c.salvar_rascunho(nova).expect("rascunho");
        c.ativar(&v.regra, v.versao, &Quem::Usuario, None)
            .expect("ativar");
    }

    /// 5 propostas aceitas seguidas e a consolidação pelo usuário (D4).
    fn consolidar(&self, regra: &str, texto: &str) {
        let mut c = self.catalogo();
        let motor = Motor::das_vigentes(&c, None).expect("motor");
        for i in 0..5 {
            let id = format!("{:064x}", 1000 + i);
            let e = entrada_do_documento(
                &id,
                "x.txt",
                "text/plain",
                Some(texto),
                Canal::Pasta,
                None,
                None,
            );
            let Decisao::Reconhecido(rec) = motor.ler(&e).decisao else {
                panic!("a regra devia reconhecer");
            };
            c.registrar_proposta(&rec, &id).expect("proposta");
            c.registrar_retorno(&rec.chave, Retorno::Aceito, &Quem::Usuario, "")
                .expect("retorno");
        }
        c.consolidar(regra, 1, &Quem::Usuario).expect("consolidar");
        assert_eq!(c.versao(regra, 1).expect("v").estado, Estado::Consolidada);
    }
}

fn extracao(regex: &str, formato: Formato) -> Extracao {
    Extracao {
        regex: regex.to_owned(),
        grupo: 1,
        formato,
        fonte: FonteDaExtracao::Texto,
    }
}

fn nova(regra: &str, definicao: Definicao) -> NovaVersao {
    NovaVersao {
        regra: regra.to_owned(),
        nome: regra.to_owned(),
        definicao,
        script: None,
        autor: AutorDaRegra::Usuario,
        motivo: "teste".to_owned(),
    }
}

fn regra_do_comprovante() -> NovaVersao {
    let mut d = Definicao::simples("comprovante", "comprovante pix");
    d.detectar
        .todos
        .push(Condicao::Contem("banco exemplo".into()));
    d.extrair.insert(
        "valor_centavos".into(),
        extracao(r"Valor:\s*R\$\s*([\d.,]+)", Formato::Centavos),
    );
    d.extrair.insert(
        "data".into(),
        extracao(r"Data:\s*(\d{2}/\d{2}/\d{4})", Formato::Data),
    );
    d.extrair.insert(
        "conta".into(),
        extracao(r"Conta de origem:\s*(.+)", Formato::Texto),
    );
    d.obrigatorios = vec!["valor_centavos".into(), "data".into()];
    d.propor = vec![ModeloDeProposta::ProvarRegistro { janela_dias: 2 }];
    nova("comprovante-pix-banco-exemplo", d)
}

fn regra_do_boleto() -> NovaVersao {
    let mut d = Definicao::simples("boleto", "boleto energia exemplo");
    d.extrair.insert(
        "valor_centavos".into(),
        extracao(r"Valor R\$\s*([\d.,]+)", Formato::Centavos),
    );
    d.extrair.insert(
        "data".into(),
        extracao(r"Vencimento (\d{2}/\d{2}/\d{4})", Formato::Data),
    );
    d.propor = vec![ModeloDeProposta::ConciliarOcorrencia {
        janela_dias: 10,
        compromisso: Some("Energia".into()),
    }];
    nova("boleto-energia-exemplo", d)
}

fn regra_da_fatura() -> NovaVersao {
    let mut d = Definicao::simples("fatura-de-cartao", "fatura cartao exemplo");
    d.extrair.insert(
        "valor_centavos".into(),
        extracao(r"Total da fatura:\s*R\$\s*([\d.,]+)", Formato::Centavos),
    );
    d.extrair.insert(
        "vencimento".into(),
        extracao(r"Vencimento:\s*(\d{2}/\d{2}/\d{4})", Formato::Data),
    );
    d.extrair
        .insert("cartao".into(), extracao(r"Cartao:\s*(.+)", Formato::Texto));
    d.propor = vec![ModeloDeProposta::PagarFatura];
    nova("fatura-cartao-exemplo", d)
}

fn de_regra(r: &Resultado) -> &Sugestao {
    r.sugestoes
        .iter()
        .find(|s| s.da_regra().is_some())
        .unwrap_or_else(|| panic!("nenhuma sugestão de regra: {:?}", r.sugestoes))
}

fn provas(s: &Sugestao) -> u8 {
    match s {
        Sugestao::ProvarRegistro { provas, .. }
        | Sugestao::ConciliarOcorrencia { provas, .. }
        | Sugestao::PagarFatura { provas, .. } => *provas,
        outra => panic!("{outra:?}"),
    }
}

/// O que o clique em Confirmar faz: aplica as alterações da sugestão.
fn confirmar(plan: &mut PlanejamentoSqlite, r: &Resultado, s: &Sugestao) {
    for a in s.alteracoes(&r.sha256, &r.nome, 0, None) {
        plan.aplicar(a, Autor::Usuario).expect("aplicar");
    }
}

fn novembro() -> Periodo {
    Periodo::MesCivil(comp(2026, 11))
}

fn hoje() -> sbf_planejamento::Data {
    sbf_planejamento::Data::hoje_em_sao_paulo()
}

// ---------------------------------------------------------------- AC-LUA-06

#[test]
fn ac_lua_06_regra_ativa_so_propoe_e_nada_grava_sozinho() {
    let c = cenario();
    c.criar("Energia", Natureza::Saida, 18_750, 24, Some("CX-0001"));
    c.mapear_conta();
    c.movimento(
        "pluggy:txn_1",
        "debito",
        "2026-11-24",
        -18_750,
        "PAGTO ENERGIA",
    );
    c.instalar(regra_do_boleto());
    let arquivo = c.solto("boleto.txt", BOLETO_DE_ENERGIA);

    let r = c.rodar(&c.com_regras(), &arquivo);

    let s = de_regra(&r);
    assert!(matches!(s, Sugestao::ConciliarOcorrencia { .. }));
    assert_eq!(provas(s), 3, "documento, valor exato e conta");
    assert!(!s.e_automatica(), "regra ativa nunca é automática");
    assert!(s.conciliacao_automatica().is_none());
    // O reconhecimento não escreveu no planejamento: nenhuma conciliação, nenhum documento.
    let plan = c.plan();
    let ocorrencias = plan
        .controle(&novembro(), Filtro::Todos, None, hoje())
        .unwrap();
    assert!(ocorrencias.linhas.iter().all(|o| o.conciliado.is_none()));
    // A proposta ficou aberta no catálogo.
    let abertas = c
        .catalogo()
        .propostas(Some(EstadoDaProposta::Aberta))
        .unwrap();
    assert_eq!(abertas.len(), 1);
    // O retorno do usuário alimenta a consolidação.
    let (_, chave) = s.da_regra().unwrap();
    regras::registrar_retorno(&c.cofre, chave, Retorno::Aceito).unwrap();
    assert_eq!(
        c.catalogo()
            .versao("boleto-energia-exemplo", 1)
            .unwrap()
            .aceitas_seguidas,
        1
    );
}

#[test]
fn ac_lua_06_consolidada_com_duas_provas_nao_e_automatica_e_com_tres_e() {
    let c = cenario();
    c.criar("Energia", Natureza::Saida, 18_750, 24, Some("CX-0001"));
    c.movimento(
        "pluggy:txn_1",
        "debito",
        "2026-11-24",
        -18_750,
        "PAGTO ENERGIA",
    );
    c.instalar(regra_do_boleto());
    c.consolidar("boleto-energia-exemplo", BOLETO_DE_ENERGIA);

    // Sem o par de conta confirmado, só o documento e o valor batem: 2 provas.
    let arquivo = c.solto("boleto.txt", BOLETO_DE_ENERGIA);
    let r = c.rodar(&c.com_regras(), &arquivo);
    let s = de_regra(&r);
    assert_eq!(provas(s), 2);
    assert!(
        !s.e_automatica(),
        "consolidada com 2 provas não concilia sozinha"
    );

    // Com o par de conta, são 3: automática, e o autor é "<regra>@<versão>".
    c.mapear_conta();
    let outro = c.solto("outro.txt", &format!("{BOLETO_DE_ENERGIA}\n"));
    let r = c.rodar(&c.com_regras(), &outro);
    let s = de_regra(&r);
    assert_eq!(provas(s), 3);
    assert!(s.e_automatica());
    let (_, autor) = s.conciliacao_automatica().expect("automática");
    assert_eq!(autor, "boleto-energia-exemplo@1");
    // Mesmo automática, o reconhecimento só sugere: quem grava é a janela.
    let plan = c.plan();
    let o = plan
        .controle(&novembro(), Filtro::Todos, None, hoje())
        .unwrap();
    assert!(o.linhas.iter().all(|l| l.conciliado.is_none()));
}

#[test]
fn ac_lua_06_nao_ha_proposta_que_crie_gasto() {
    // Não existe variante de `Proposta` nem de `Sugestao` que crie recorrente ou registro:
    // aplicar qualquer sugestão de regra só concilia ou vincula.
    let c = cenario();
    c.criar("Energia", Natureza::Saida, 18_750, 24, Some("CX-0001"));
    c.mapear_conta();
    c.movimento(
        "pluggy:txn_1",
        "debito",
        "2026-11-24",
        -18_750,
        "PAGTO ENERGIA",
    );
    c.instalar(regra_do_boleto());
    let arquivo = c.solto("boleto.txt", BOLETO_DE_ENERGIA);
    let r = c.rodar(&c.com_regras(), &arquivo);
    for a in de_regra(&r).alteracoes(&r.sha256, &r.nome, 0, None) {
        assert!(matches!(a, Alteracao::Conciliar { .. }), "{a:?}");
    }
}

// ---------------------------------------------------------------- AC-LUA-09

#[test]
fn ac_lua_09_arrasto_e_depois_ja_no_cofre_e_uma_proposta_so() {
    let c = cenario();
    c.instalar(regra_do_comprovante());
    let r = c.com_regras();
    let arrastado = c.solto("comprovante.txt", COMPROVANTE);

    let primeira = c.rodar(&r, &arrastado);
    assert_eq!(
        primeira.entrada.as_ref().unwrap().origem.canal,
        Canal::Arrastado
    );

    // A cópia que o arrasto guardou no acervo do cofre.
    let copia = achar_copia(&c.cofre, COMPROVANTE);
    let segunda = c.rodar(&r, &copia);
    let e = segunda.entrada.as_ref().unwrap();
    assert_eq!(e.origem.canal, Canal::Cofre);
    assert_eq!(
        e.id,
        primeira.entrada.as_ref().unwrap().id,
        "o id é o sha256"
    );

    let propostas = c.catalogo().propostas(None).unwrap();
    assert_eq!(propostas.len(), 1, "a mesma chave: uma proposta só");
}

fn achar_copia(raiz: &Path, conteudo: &str) -> PathBuf {
    fn andar(pasta: &Path, conteudo: &[u8], achados: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(pasta).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                andar(&p, conteudo, achados);
            } else if std::fs::read(&p).is_ok_and(|b| b == conteudo) {
                achados.push(p);
            }
        }
    }
    let mut achados = Vec::new();
    andar(raiz, conteudo.as_bytes(), &mut achados);
    achados.pop().expect("a cópia no acervo")
}

#[test]
fn ac_lua_09_mesmo_conteudo_em_dois_caminhos_acumula_origens_e_uma_proposta() {
    let c = cenario();
    c.instalar(regra_do_comprovante());
    let r = c.com_regras();
    c.rodar(&r, &c.solto("a.txt", COMPROVANTE));
    c.rodar(&r, &c.solto("b.txt", COMPROVANTE));
    assert_eq!(c.contar("SELECT COUNT(*) FROM achado"), 1);
    assert_eq!(
        c.contar("SELECT COUNT(*) FROM origem"),
        2,
        "o inventário acumula as origens"
    );
    assert_eq!(c.catalogo().propostas(None).unwrap().len(), 1);
}

#[test]
fn ac_lua_09_telegram_e_pasta_com_o_mesmo_sha_dao_a_mesma_chave() {
    let c = cenario();
    c.instalar(regra_do_comprovante());
    let mut cat = c.catalogo();
    let motor = Motor::das_vigentes(&cat, None).unwrap();
    let sha = "ab".repeat(32);
    let ler = |canal, id: &str| {
        let e = entrada_do_documento(
            &sha,
            "c.pdf",
            "application/pdf",
            Some(COMPROVANTE),
            canal,
            Some(id),
            None,
        );
        match motor.ler(&e).decisao {
            Decisao::Reconhecido(rec) => *rec,
            outra => panic!("{outra:?}"),
        }
    };
    let telegram = ler(Canal::Telegram, "msg-77");
    let pasta = ler(Canal::Pasta, "/pasta/c.pdf");
    assert_eq!(telegram.chave, pasta.chave);
    assert!(cat.registrar_proposta(&telegram, &sha).unwrap());
    assert!(!cat.registrar_proposta(&pasta, &sha).unwrap(), "já existia");
    assert_eq!(cat.propostas(None).unwrap().len(), 1);
}

#[test]
fn ac_lua_09_o_mesmo_registro_da_pluggy_lido_duas_vezes_e_uma_proposta() {
    let c = cenario();
    let mut cat = c.catalogo();
    cat.semear_fabrica().unwrap();
    // A regra de fábrica do Pix chega em rascunho; ativar é decisão do usuário.
    cat.ativar("pix-na-descricao", 1, &Quem::Usuario, None)
        .unwrap();
    let motor = Motor::das_vigentes(&cat, None).unwrap();
    let m = movimento_pix();
    let e = entrada_do_registro(&m);
    assert_eq!(e.id, "pluggy:txn_pix");
    let primeira = ler_registro(&motor, &e).expect("reconhece o Pix");
    let segunda = ler_registro(&motor, &entrada_do_registro(&m)).expect("de novo");
    assert_eq!(primeira.chave, segunda.chave);
    assert!(cat.registrar_proposta(&primeira, &e.id).unwrap());
    assert!(!cat.registrar_proposta(&segunda, &e.id).unwrap());
    assert_eq!(cat.propostas(None).unwrap().len(), 1);
}

fn movimento_pix() -> sbf_planejamento::MovimentoDoExtrato {
    sbf_planejamento::MovimentoDoExtrato {
        movimento: "pluggy:txn_pix".into(),
        data: sbf_planejamento::Data::nova(2026, 11, 12).unwrap(),
        valor: Centavos(-18_750),
        conta_do_extrato: CONTA.into(),
        descricao: "PIX ENVIADO Fulano de Tal".into(),
        tipo: "debito".into(),
        contraparte: None,
        contraparte_doc: None,
    }
}

#[test]
fn d13_favorecido_e_tipo_so_enriquecem_o_rotulo_do_candidato() {
    let c = cenario();
    let mut cat = c.catalogo();
    cat.semear_fabrica().unwrap();
    cat.ativar("pix-na-descricao", 1, &Quem::Usuario, None)
        .unwrap();
    let motor = Motor::das_vigentes(&cat, None).unwrap();
    let candidato = sbf_planejamento::MovimentoCandidato {
        movimento: movimento_pix(),
        bate_assinatura: false,
        mesma_conta: true,
        diferenca: None,
    };
    let rotulo = rotulo_do_candidato(&motor, &candidato);
    assert!(rotulo.starts_with("PIX ENVIADO Fulano de Tal"), "{rotulo}");
    assert!(rotulo.contains("favorecido Fulano de Tal"), "{rotulo}");
    assert!(rotulo.ends_with("pix"), "{rotulo}");
    // Sem a regra vigente, o rótulo é o de sempre.
    let vazio = Motor::novo(Vec::new(), Vec::new(), None);
    assert_eq!(
        rotulo_do_candidato(&vazio, &candidato),
        "PIX ENVIADO Fulano de Tal"
    );
}

#[test]
fn ambiguo_vai_para_precisa_revisar_e_nao_grava_proposta() {
    let c = cenario();
    let mut a = Definicao::simples("comprovante", "comprovante pix");
    a.extrair.insert(
        "valor_centavos".into(),
        extracao(r"Valor:\s*R\$\s*([\d.,]+)", Formato::Centavos),
    );
    let mut b = Definicao::simples("comprovante", "favorecido");
    b.extrair.insert(
        "valor_centavos".into(),
        extracao(r"Tarifa:\s*R\$\s*([\d.,]+)", Formato::Centavos),
    );
    c.instalar(nova("regra-a", a));
    c.instalar(nova("regra-b", b));
    let arquivo = c.solto("c.txt", &format!("{COMPROVANTE}Tarifa: R$ 2,00\n"));

    let r = c.rodar(&c.com_regras(), &arquivo);

    match &r.reconhecido {
        Reconhecido::Ambiguo { candidatas, motivo } => {
            assert_eq!(candidatas.len(), 2);
            assert!(motivo.contains("valor_centavos"), "{motivo}");
        }
        outra => panic!("{outra:?}"),
    }
    assert!(r.sugestoes.is_empty());
    assert!(c.catalogo().propostas(None).unwrap().is_empty());
    assert!(matches!(
        r.leitura.as_ref().unwrap().decisao,
        Decisao::Ambiguo { .. }
    ));
}

// ---------------------------------------------------------------- AC-LUA-10

#[test]
fn ac_lua_10_fatura_mais_debito_concilia_a_ocorrencia_sem_gasto_novo() {
    let c = cenario();
    c.criar(
        "Cartão Exemplo",
        Natureza::Saida,
        90_000,
        10,
        Some("CX-0001"),
    );
    c.criar(
        "Compras no Cartão Exemplo",
        Natureza::DentroDaFatura,
        30_000,
        3,
        None,
    );
    c.mapear_conta();
    c.movimento(
        "pluggy:txn_fat",
        "pagamento_fatura",
        "2026-11-09",
        -90_000,
        "PAGTO FATURA",
    );
    c.instalar(regra_da_fatura());
    let antes_registros = c.contar("SELECT COUNT(*) FROM registro");
    let antes = c
        .plan()
        .controle(&novembro(), Filtro::Todos, None, hoje())
        .unwrap();
    let arquivo = c.solto("fatura.txt", FATURA);

    let r = c.rodar(&c.com_regras(), &arquivo);

    let s = de_regra(&r);
    let Sugestao::PagarFatura { conciliacao, .. } = s else {
        panic!("{s:?}");
    };
    assert_eq!(conciliacao.movimento, "pluggy:txn_fat");
    assert_eq!(conciliacao.valor, Centavos(90_000));
    assert_eq!(provas(s), 3);
    assert!(s.rotulo().contains("não é gasto novo"));

    let mut plan = c.plan();
    confirmar(&mut plan, &r, s);
    let depois = plan
        .controle(&novembro(), Filtro::Todos, None, hoje())
        .unwrap();
    let fatura_depois = depois
        .linhas
        .iter()
        .find(|o| o.nome == "Cartão Exemplo")
        .unwrap();
    assert_eq!(
        fatura_depois.conciliado,
        Some(Centavos(90_000)),
        "a fatura ficou conciliada"
    );
    // Nenhum gasto novo: os números do Controle mensal e o extrato são os mesmos.
    assert_eq!(
        depois.resumo.saidas_de_caixa_previstas,
        antes.resumo.saidas_de_caixa_previstas
    );
    assert_eq!(
        depois.resumo.compras_no_cartao,
        antes.resumo.compras_no_cartao
    );
    assert_eq!(depois.linhas.len(), antes.linhas.len());
    assert_eq!(c.contar("SELECT COUNT(*) FROM registro"), antes_registros);
}

#[test]
fn ac_lua_10_comprovante_prova_o_debito_que_ja_existe_sem_registro_novo() {
    let c = cenario();
    let id = c.criar("Energia", Natureza::Saida, 18_750, 12, Some("CX-0001"));
    c.mapear_conta();
    c.movimento(
        "pluggy:txn_1",
        "debito",
        "2026-11-12",
        -18_750,
        "PIX ENVIADO",
    );
    c.instalar(regra_do_comprovante());
    // O débito já está conciliado à ocorrência: é nela que o comprovante se anexa.
    let ocorrencia = sbf_planejamento::IdOcorrencia {
        recorrente: id,
        competencia: comp(2026, 11),
        parcela: None,
    };
    c.plan()
        .aplicar(
            Alteracao::Conciliar {
                conciliacao: sbf_planejamento::Conciliacao {
                    ocorrencia: ocorrencia.clone(),
                    movimento: "pluggy:txn_1".into(),
                    valor: Centavos(18_750),
                    data: sbf_planejamento::Data::nova(2026, 11, 12).unwrap(),
                    confirmada: true,
                },
            },
            Autor::Usuario,
        )
        .unwrap();
    let registros = c.contar("SELECT COUNT(*) FROM registro");
    let arquivo = c.solto("comprovante.txt", COMPROVANTE);

    let r = c.rodar(&c.com_regras(), &arquivo);

    let s = de_regra(&r);
    let Sugestao::ProvarRegistro {
        movimento, alvo, ..
    } = s
    else {
        panic!("{s:?}");
    };
    assert_eq!(movimento.movimento, "pluggy:txn_1");
    assert_eq!(
        alvo,
        &Some(AlvoDoDocumento::Ocorrencia {
            id: ocorrencia.clone()
        })
    );
    assert_eq!(provas(s), 3);
    assert!(!s.e_automatica(), "regra ativa, não consolidada");

    let mut plan = c.plan();
    confirmar(&mut plan, &r, s);
    let alvo = AlvoDoDocumento::Ocorrencia { id: ocorrencia };
    assert_eq!(
        plan.documentos(&alvo).unwrap().len(),
        1,
        "o comprovante ficou ligado"
    );
    assert_eq!(
        c.contar("SELECT COUNT(*) FROM registro"),
        registros,
        "nenhum registro novo"
    );
    // Aplicar de novo não duplica: a sugestão já diz que está ligado.
    let de_novo = c.rodar(&c.com_regras(), &c.solto("copia.txt", COMPROVANTE));
    assert!(de_novo.sugestoes.iter().all(|s| s.da_regra().is_none()));
    assert!(de_novo.avisos.iter().any(|a| a.contains("já está ligado")));
}

#[test]
fn ac_lua_10_comprovante_sem_debito_unico_nao_inventa_um() {
    let c = cenario();
    c.movimento("pluggy:txn_1", "debito", "2026-11-12", -18_750, "A");
    c.movimento("pluggy:txn_2", "debito", "2026-11-12", -18_750, "B");
    c.instalar(regra_do_comprovante());
    let r = c.rodar(&c.com_regras(), &c.solto("c.txt", COMPROVANTE));
    // Dois débitos iguais na mesma conta: empate não se adivinha.
    assert!(r.sugestoes.is_empty());
    assert!(
        r.avisos.iter().any(|a| a.contains("único débito")),
        "{:?}",
        r.avisos
    );
}

#[test]
fn ac_lua_10_contracheque_liquido_e_desconto_em_folha_nao_reduzem_o_caixa_duas_vezes() {
    // O Controle mensal já prova: `sbf-planejamento/tests/aceite.rs` (desconto em folha fora
    // das saídas de caixa) e `tests/salario.rs` (o previsto do desconto segue a rubrica).
    // Aqui, de ponta a ponta com o reconhecimento por regras: o contracheque de 2026 cria os
    // descontos em folha e as saídas de caixa seguem em zero.
    let c = cenario();
    let id = c.criar("Salário", Natureza::Entrada, 2_000_000, 5, Some("CX-0001"));
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE_2026);
    let r = c.rodar(&c.com_regras(), &arquivo);
    assert!(matches!(r.reconhecido, Reconhecido::Contracheque { .. }));

    let mut plan = c.plan();
    for s in &r.sugestoes {
        if let Sugestao::CriarDescontosEmFolha { uso, descontos, .. } = s {
            sbf_reconhecimento::descontos::aplicar_descontos(
                &mut plan,
                uso,
                descontos,
                &uso.eventuais_iniciais(),
            )
            .unwrap();
            continue;
        }
        for mut a in s.alteracoes(&r.sha256, &r.nome, 0, None) {
            if let Alteracao::DefinirComposicao { versao_base, .. } = &mut a {
                *versao_base = plan.recorrente(&id).unwrap().unwrap().versao_do_registro;
            }
            plan.aplicar(a, Autor::Usuario).unwrap();
        }
    }
    let c11 = plan
        .controle(&novembro(), Filtro::Todos, None, hoje())
        .unwrap();
    assert!(
        c11.resumo.descontos_em_folha.0 > 0,
        "os descontos em folha aparecem"
    );
    assert_eq!(
        c11.resumo.saidas_de_caixa_previstas,
        Centavos(0),
        "e não saem do caixa de novo: o líquido já vem descontado"
    );
}

// ---------------------------------------------------------------- AC-LUA-11

fn ativar_a_versao_2_do_contracheque(c: &Cenario, executor: &ExecutorNoProcesso) {
    let mut cat = c.catalogo();
    cat.semear_fabrica().unwrap();
    assert_eq!(
        cat.versao("contracheque", 2).unwrap().estado,
        Estado::Rascunho
    );
    cat.ativar("contracheque", 2, &Quem::Usuario, Some(executor))
        .unwrap();
}

#[test]
fn ac_lua_11_irrf_estimado_traz_a_tabela_e_alerta_sem_mexer_na_rubrica() {
    let c = cenario();
    let executor = ExecutorNoProcesso::novo();
    ativar_a_versao_2_do_contracheque(&c, &executor);
    let r = ReconhecedorDoCofre::novo(&c.cofre, Arc::new(Segredos), None)
        .com_regras(Some(Arc::new(executor)));
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE_2026);

    let resultado = c.rodar(&r, &arquivo);

    // A base de cálculo de R$ 18.000,00 dá R$ 4.041,27 pela tabela; o contracheque reteve
    // R$ 6.123,40 (sem a do PLR).
    let aviso = resultado
        .avisos
        .iter()
        .find(|a| a.contains("IRRF"))
        .unwrap_or_else(|| panic!("sem aviso do IRRF: {:?}", resultado.avisos));
    assert_eq!(
        aviso,
        "⚠ IRRF estimado R$ 4.041,27 difere do retido R$ 6.123,40 (irrf-2026-01)"
    );
    // O Rastro traz o reconhecimento completo, com a tabela e a versão nas evidências.
    let leitura = resultado.leitura.as_ref().unwrap();
    let Decisao::Reconhecido(rec) = &leitura.decisao else {
        panic!("{:?}", leitura.decisao);
    };
    assert_eq!((rec.regra.as_str(), rec.versao), ("contracheque", 2));
    assert!(rec.revisar);
    assert!(
        rec.evidencias
            .iter()
            .any(|e| e == "tabela:irrf:irrf-2026-01")
    );
    assert!(rec.evidencias.iter().any(|e| e == "rubrica_irrf"));
    assert_eq!(
        rec.campos["irrf_estimado_centavos"].valor,
        Valor::Centavos(404_127)
    );
    assert_eq!(
        rec.campos["irrf_retido_centavos"].valor,
        Valor::Centavos(612_340)
    );
    assert_eq!(
        rec.campos["irrf_diferenca_centavos"].valor,
        Valor::Centavos(208_213)
    );
    // Nada do contracheque mudou: as rubricas são as que o leitor nativo lê.
    let Reconhecido::Contracheque { contracheque, .. } = &resultado.reconhecido else {
        panic!("{:?}", resultado.reconhecido);
    };
    let nativo = sbf_planilha::contracheque::ler(CONTRACHEQUE_2026).unwrap();
    assert_eq!(contracheque.rubricas, nativo.rubricas);
    assert_eq!(contracheque.liquido, nativo.liquido);
}

#[test]
fn ac_lua_11_com_a_v1_ativa_nao_ha_alerta_de_irrf() {
    let c = cenario();
    let r = c.rodar(&c.com_regras(), &c.solto("c.txt", CONTRACHEQUE_2026));
    assert!(r.avisos.iter().all(|a| !a.contains("IRRF")));
    let leitura = r.leitura.as_ref().unwrap();
    let Decisao::Reconhecido(rec) = &leitura.decisao else {
        panic!();
    };
    assert_eq!((rec.regra.as_str(), rec.versao), ("contracheque", 1));
    assert_eq!(rec.estado, Estado::Ativa);
    assert_eq!(
        rec.campos["liquido_centavos"].valor,
        Valor::Centavos(2_460_522)
    );
    assert_eq!(
        rec.campos["competencia"].valor,
        Valor::Competencia("2026-10".into())
    );
    assert_eq!(
        rec.campos["base_irrf_centavos"].valor,
        Valor::Centavos(1_800_000)
    );
    assert_eq!(
        rec.campos["irrf_retido_centavos"].valor,
        Valor::Centavos(612_340)
    );
}

// ---------------------------------------------------------------- AC-LUA-14

fn sem_o_rastro(mut r: Resultado) -> Resultado {
    r.entrada = None;
    r.leitura = None;
    r
}

#[test]
fn ac_lua_14_com_e_sem_regras_o_resultado_atual_e_o_mesmo() {
    for (nome, texto) in [
        ("contracheque.txt", CONTRACHEQUE_2026),
        (
            "boleto.txt",
            "BOLETO DE COBRANCA ENERGIA EXEMPLO Valor R$ 187,50",
        ),
        ("anotacao.txt", "nada financeiro aqui"),
    ] {
        let sem = cenario();
        let com = cenario();
        for c in [&sem, &com] {
            c.criar("Salário", Natureza::Entrada, 2_000_000, 5, Some("CX-0001"));
        }
        let r_sem = sem.rodar(&sem.sem_regras(), &sem.solto(nome, texto));
        let r_com = com.rodar(&com.com_regras(), &com.solto(nome, texto));
        assert!(r_com.entrada.is_some() && r_com.leitura.is_some(), "{nome}");
        assert_eq!(r_sem.entrada, None);
        // Os ids dos recorrentes são sorteados por cofre; o resto tem de ser idêntico.
        let r_com = sem_o_rastro(r_com);
        assert_eq!(r_com.reconhecido, r_sem.reconhecido, "{nome}");
        assert_eq!(r_com.texto, r_sem.texto, "{nome}");
        assert_eq!(r_com.avisos, r_sem.avisos, "{nome}");
        assert_eq!(r_com.sha256, r_sem.sha256, "{nome}");
        let rotulos = |r: &Resultado| {
            r.sugestoes
                .iter()
                .map(|s| s.rotulo().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(rotulos(&r_com), rotulos(&r_sem), "{nome}");
    }
}

#[test]
fn ac_lua_14_se_o_catalogo_nao_abre_o_caminho_atual_roda_como_hoje() {
    let sem = cenario();
    let com = cenario();
    // `regras.sqlite` é uma pasta: o catálogo não abre.
    std::fs::create_dir(com.cofre.join(ARQUIVO_DAS_REGRAS)).unwrap();
    let r_sem = sem.rodar(&sem.sem_regras(), &sem.solto("c.txt", CONTRACHEQUE_2026));
    let r_com = com.rodar(&com.com_regras(), &com.solto("c.txt", CONTRACHEQUE_2026));
    assert_eq!(r_com.entrada, None);
    assert_eq!(r_com, r_sem);
}

#[test]
fn a_entrada_leva_o_mime_pela_assinatura_e_o_canal_do_cofre() {
    assert_eq!(
        regras::mime_pelo_conteudo(b"%PDF-1.7 ..."),
        "application/pdf"
    );
    assert_eq!(
        regras::mime_pelo_conteudo(&[0x89, b'P', b'N', b'G', 1]),
        "image/png"
    );
    assert_eq!(
        regras::mime_pelo_conteudo(&[0xFF, 0xD8, 0xFF, 0xE0]),
        "image/jpeg"
    );
    assert_eq!(
        regras::mime_pelo_conteudo("olá, mundo".as_bytes()),
        "text/plain"
    );
    assert_eq!(
        regras::mime_pelo_conteudo(&[0, 1, 2, 3]),
        "application/octet-stream"
    );
    let c = cenario();
    let dentro = c.cofre.join("anotacao.txt");
    std::fs::write(&dentro, "x").unwrap();
    let r = c.rodar(&c.com_regras(), &dentro);
    let e = r.entrada.expect("entrada");
    assert_eq!(e.origem.canal, Canal::Cofre);
    assert_eq!(e.mime.as_deref(), Some("text/plain"));
    assert_eq!(e.nome.as_deref(), Some("anotacao.txt"));
}

const CONTA_DE_AGUA: &str =
    "Conta de Agua Exemplo\nContrato 998877-6\nVencimento em 25 de Novembro\nValor R$ 89,90\n";

fn regra_da_conta_de_agua() -> NovaVersao {
    let mut d = Definicao::simples("conta-agua-exemplo", "conta de agua exemplo");
    d.extrair.insert(
        "competencia".into(),
        extracao(
            r"Vencimento em (\d{1,2} de [A-Za-zç]+)",
            Formato::Competencia,
        ),
    );
    d.obrigatorios.push("competencia".into());
    nova("conta-agua-exemplo", d)
}

/// Spec 15: a competência sem ano ("Vencimento em 25 de Novembro") sai do `momento` do
/// documento, que o reconhecimento tira do inventário (a menor data de origem).
#[test]
fn competencia_sem_ano_usa_o_momento_do_inventario() {
    let c = cenario();
    c.instalar(regra_da_conta_de_agua());
    let r = c.com_regras();
    let arrastado = c.solto("conta-agua.txt", CONTA_DE_AGUA);

    // A primeira leitura registra o achado e a origem; a data do arquivo passa a ser a do
    // inventário (a de 18/11, mais antiga que a do arquivo recém-escrito).
    let primeira = c.rodar(&r, &arrastado);
    c.conexao()
        .execute(
            "UPDATE origem SET local_mtime = '2026-11-18T12:00:00.000Z' WHERE content_hash = ?1",
            [&primeira.sha256],
        )
        .expect("data da origem");

    let segunda = c.rodar(&r, &arrastado);
    let entrada = segunda.entrada.as_ref().expect("entrada");
    assert_eq!(
        entrada.momento.as_deref(),
        Some("2026-11-18T12:00:00.000Z"),
        "o momento é a menor data de origem"
    );
    let Reconhecido::PelaRegra { reconhecimento } = &segunda.reconhecido else {
        panic!("a regra devia reconhecer: {:?}", segunda.reconhecido);
    };
    assert_eq!(
        reconhecimento.campos["competencia"].valor,
        Valor::Competencia("2026-11".into())
    );
}

// ---------------------------------------------------------------- vincular_ocorrencia

const INVOICE_DA_NUVEM: &str = "Nuvem Exemplo Inc\nInvoice\nInvoice number  NE-0001\n\
Date of issue  November 20, 2026\n$4.00 USD due November 20, 2026\n";
const RECEIPT_DA_NUVEM: &str = "Nuvem Exemplo Inc\nReceipt\nReceipt number  NE-0001-R\n\
Date paid  November 20, 2026\n$4.00 USD paid\n";

/// Uma regra em inglês que só liga o documento à ocorrência do mês (sem valor, sem extrato).
fn regra_da_nuvem(
    id: &str,
    titulo: &str,
    regex_da_data: &str,
    tipo: sbf_regras::TipoDoDocumento,
    janela_dias: u32,
) -> NovaVersao {
    let mut d = Definicao::simples(id, titulo);
    d.detectar
        .todos
        .push(Condicao::Contem("nuvem exemplo".into()));
    d.extrair
        .insert("data".into(), extracao(regex_da_data, Formato::Data));
    d.obrigatorios = vec!["data".into()];
    d.propor = vec![ModeloDeProposta::VincularOcorrencia {
        compromisso: "Nuvem Exemplo".into(),
        tipo_documento: tipo,
        janela_dias,
    }];
    nova(id, d)
}

fn regra_da_invoice() -> NovaVersao {
    regra_da_nuvem(
        "invoice-nuvem-exemplo",
        "invoice",
        r"Date of issue\s+([A-Za-z]+ \d{1,2}, \d{4})",
        sbf_regras::TipoDoDocumento::Fatura,
        20,
    )
}

fn regra_do_receipt() -> NovaVersao {
    regra_da_nuvem(
        "receipt-nuvem-exemplo",
        "receipt",
        r"Date paid\s+([A-Za-z]+ \d{1,2}, \d{4})",
        sbf_regras::TipoDoDocumento::Comprovante,
        20,
    )
}

fn ocorrencia_de(id: String, ano: i32, mes: u8) -> AlvoDoDocumento {
    AlvoDoDocumento::Ocorrencia {
        id: sbf_planejamento::IdOcorrencia {
            recorrente: id,
            competencia: comp(ano, mes),
            parcela: None,
        },
    }
}

fn so_vincular(r: &Resultado) -> &Sugestao {
    r.sugestoes
        .iter()
        .find(|s| matches!(s, Sugestao::Vincular { regra: Some(_), .. }))
        .unwrap_or_else(|| panic!("nenhum vincular de regra: {:?} {:?}", r.sugestoes, r.avisos))
}

#[test]
fn vincular_ocorrencia_liga_a_invoice_e_o_receipt_em_ingles_a_mesma_ocorrencia() {
    let c = cenario();
    let id = c.criar("Nuvem Exemplo PRO", Natureza::Saida, 2_200, 20, None);
    c.instalar(regra_da_invoice());
    c.instalar(regra_do_receipt());
    let alvo = ocorrencia_de(id, 2026, 11);

    let invoice = c.rodar(&c.com_regras(), &c.solto("invoice.txt", INVOICE_DA_NUVEM));
    let s = so_vincular(&invoice);
    let Sugestao::Vincular {
        alvo: a,
        rotulo,
        tipo,
        data,
        ..
    } = s
    else {
        panic!("{s:?}");
    };
    assert_eq!(a, &alvo);
    assert_eq!(rotulo, "Vincular a Nuvem Exemplo PRO · 11/2026");
    assert_eq!(*tipo, sbf_planejamento::TipoDeDocumento::Fatura);
    assert_eq!(*data, sbf_planejamento::Data::nova(2026, 11, 20));
    assert!(!s.e_automatica());
    assert!(
        s.da_regra().is_some(),
        "o retorno do usuário vai para a regra"
    );

    let mut plan = c.plan();
    confirmar(&mut plan, &invoice, s);
    let receipt = c.rodar(&c.com_regras(), &c.solto("receipt.txt", RECEIPT_DA_NUVEM));
    let s = so_vincular(&receipt);
    assert!(matches!(
        s,
        Sugestao::Vincular {
            tipo: sbf_planejamento::TipoDeDocumento::Comprovante,
            ..
        }
    ));
    confirmar(&mut plan, &receipt, s);

    let documentos = plan.documentos(&alvo).unwrap();
    assert_eq!(
        documentos.len(),
        2,
        "os dois documentos na mesma ocorrência"
    );
    assert!(documentos.iter().any(|d| d.sha256 == invoice.sha256));
    assert!(documentos.iter().any(|d| d.sha256 == receipt.sha256));
}

#[test]
fn vincular_ocorrencia_aceita_a_ocorrencia_ja_conciliada() {
    let c = cenario();
    let id = c.criar(
        "Nuvem Exemplo PRO",
        Natureza::Saida,
        2_200,
        20,
        Some("CX-0001"),
    );
    c.mapear_conta();
    c.movimento(
        "pluggy:txn_1",
        "debito",
        "2026-11-20",
        -2_200,
        "NUVEM EXEMPLO",
    );
    let alvo = ocorrencia_de(id, 2026, 11);
    let AlvoDoDocumento::Ocorrencia { id: ocorrencia } = alvo.clone() else {
        unreachable!()
    };
    c.plan()
        .aplicar(
            Alteracao::Conciliar {
                conciliacao: sbf_planejamento::Conciliacao {
                    ocorrencia,
                    movimento: "pluggy:txn_1".into(),
                    valor: Centavos(2_200),
                    data: sbf_planejamento::Data::nova(2026, 11, 20).unwrap(),
                    confirmada: true,
                },
            },
            Autor::Usuario,
        )
        .unwrap();
    c.instalar(regra_do_receipt());

    let r = c.rodar(&c.com_regras(), &c.solto("receipt.txt", RECEIPT_DA_NUVEM));

    let Sugestao::Vincular { alvo: a, .. } = so_vincular(&r) else {
        unreachable!()
    };
    assert_eq!(a, &alvo, "recibo em ocorrência já paga é válido");
}

#[test]
fn vincular_ocorrencia_sem_ocorrencia_na_janela_nao_sugere_nada() {
    let c = cenario();
    c.criar("Nuvem Exemplo PRO", Natureza::Saida, 2_200, 5, None);
    // A ocorrência de novembro vence dia 5; o documento é de 20/11 e a janela é de 5 dias.
    c.instalar(regra_da_nuvem(
        "invoice-nuvem-exemplo",
        "invoice",
        r"Date of issue\s+([A-Za-z]+ \d{1,2}, \d{4})",
        sbf_regras::TipoDoDocumento::Fatura,
        5,
    ));

    let r = c.rodar(&c.com_regras(), &c.solto("invoice.txt", INVOICE_DA_NUVEM));

    assert!(r.sugestoes.is_empty(), "{:?}", r.sugestoes);
    assert!(
        r.avisos.iter().any(|a| a.contains("ocorrência")),
        "o motivo aparece: {:?}",
        r.avisos
    );
}

#[test]
fn vincular_ocorrencia_de_outro_compromisso_nao_sugere_nada() {
    let c = cenario();
    c.criar("Energia", Natureza::Saida, 18_750, 20, None);
    c.instalar(regra_da_invoice());
    let r = c.rodar(&c.com_regras(), &c.solto("invoice.txt", INVOICE_DA_NUVEM));
    assert!(r.sugestoes.is_empty(), "{:?}", r.sugestoes);
}

#[test]
fn vincular_ocorrencia_com_empate_de_distancia_nao_adivinha() {
    let c = cenario();
    // 18 e 22 de novembro: as duas a 2 dias do documento (20/11).
    c.criar("Nuvem Exemplo PRO", Natureza::Saida, 2_200, 18, None);
    c.criar("Nuvem Exemplo Team", Natureza::Saida, 4_400, 22, None);
    c.instalar(regra_da_invoice());

    let r = c.rodar(&c.com_regras(), &c.solto("invoice.txt", INVOICE_DA_NUVEM));

    assert!(r.sugestoes.is_empty(), "{:?}", r.sugestoes);
    assert!(
        r.avisos.iter().any(|a| a.contains("empate")),
        "o motivo diz que empatou: {:?}",
        r.avisos
    );
}

#[test]
fn vincular_ocorrencia_escolhe_a_mais_proxima() {
    let c = cenario();
    let perto = c.criar("Nuvem Exemplo PRO", Natureza::Saida, 2_200, 18, None);
    c.criar("Nuvem Exemplo Team", Natureza::Saida, 4_400, 28, None);
    c.instalar(regra_da_invoice());

    let r = c.rodar(&c.com_regras(), &c.solto("invoice.txt", INVOICE_DA_NUVEM));

    let Sugestao::Vincular { alvo, .. } = so_vincular(&r) else {
        unreachable!()
    };
    assert_eq!(alvo, &ocorrencia_de(perto, 2026, 11));
}

#[test]
fn vincular_ocorrencia_nao_repete_o_documento_ja_vinculado() {
    let c = cenario();
    let id = c.criar("Nuvem Exemplo PRO", Natureza::Saida, 2_200, 20, None);
    c.instalar(regra_do_receipt());
    let r = c.rodar(&c.com_regras(), &c.solto("receipt.txt", RECEIPT_DA_NUVEM));
    let mut plan = c.plan();
    confirmar(&mut plan, &r, so_vincular(&r));
    assert_eq!(
        plan.documentos(&ocorrencia_de(id, 2026, 11)).unwrap().len(),
        1
    );

    // O mesmo conteúdo de novo (outro caminho): já está ligado, não sugere de novo.
    let de_novo = c.rodar(&c.com_regras(), &c.solto("copia.txt", RECEIPT_DA_NUVEM));

    assert!(
        de_novo.sugestoes.iter().all(|s| s.da_regra().is_none()),
        "{:?}",
        de_novo.sugestoes
    );
    assert!(
        de_novo.avisos.iter().any(|a| a.contains("já está ligado")),
        "{:?}",
        de_novo.avisos
    );
}

#[test]
fn vincular_ocorrencia_de_padrao_ativo_so_propoe_e_de_consolidado_vincula_sozinho() {
    use sbf_reconhecimento::automaticas::aplicar_automaticas_do_resultado;
    let c = cenario();
    let id = c.criar("Nuvem Exemplo PRO", Natureza::Saida, 2_200, 20, None);
    let alvo = ocorrencia_de(id, 2026, 11);
    c.instalar(regra_do_receipt());

    // Padrão só ativo: sugere, não é automática, e aplicar não grava nada.
    let ativo = c.rodar(&c.com_regras(), &c.solto("receipt.txt", RECEIPT_DA_NUVEM));
    assert!(!so_vincular(&ativo).e_automatica());
    let saida = aplicar_automaticas_do_resultado(&c.cofre, &ativo).unwrap();
    assert!(saida.aplicadas.is_empty());
    assert!(c.plan().documentos(&alvo).unwrap().is_empty());

    // Consolidado: automática, gravada com o autor "<regra>@<versão>".
    c.consolidar("receipt-nuvem-exemplo", RECEIPT_DA_NUVEM);
    let outro = format!("{RECEIPT_DA_NUVEM}\n");
    let r = c.rodar(&c.com_regras(), &c.solto("receipt-2.txt", &outro));
    let s = so_vincular(&r);
    assert!(s.e_automatica());
    assert_eq!(
        s.autor_do_vinculo_automatico().as_deref(),
        Some("receipt-nuvem-exemplo@1")
    );
    let saida = aplicar_automaticas_do_resultado(&c.cofre, &r).unwrap();
    assert_eq!(saida.novas(), 1);
    let docs = c.plan().documentos(&alvo).unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(
        docs[0].autor,
        Autor::Regra {
            regra: "receipt-nuvem-exemplo@1".to_owned()
        }
    );
    // Idempotente: de novo, nada novo.
    assert_eq!(
        aplicar_automaticas_do_resultado(&c.cofre, &r)
            .unwrap()
            .novas(),
        0
    );
    assert_eq!(c.plan().documentos(&alvo).unwrap().len(), 1);

    // O usuário desfaz: a regra não refaz, e a sugestão volta sem ser automática.
    let doc = docs.into_iter().next().unwrap();
    c.plan()
        .aplicar(
            Alteracao::DesvincularDocumento { id: doc.id },
            Autor::Usuario,
        )
        .unwrap();
    let depois = c.rodar(&c.com_regras(), &c.solto("receipt-2.txt", &outro));
    assert!(!so_vincular(&depois).e_automatica());
    let saida = aplicar_automaticas_do_resultado(&c.cofre, &r).unwrap();
    assert!(saida.aplicadas.is_empty());
    assert!(c.plan().documentos(&alvo).unwrap().is_empty());
}

// ------------------------------------------------------------ identidade do documento

const INVOICE_POR_EMAIL: &str = "Nuvem Exemplo Inc\nInvoice\nInvoice number  NE-0001\n\
Date of issue  November 20, 2026\n$4.00 USD due November 20, 2026\n";
/// O mesmo documento encaminhado pelo Telegram: o texto muda um pouco, o número é o mesmo.
const INVOICE_PELO_TELEGRAM: &str = "Nuvem Exemplo Inc (encaminhado)\nInvoice\n\
Invoice number   ne-0001 \nDate of issue  November 20, 2026\n$4.00 USD\n";

fn regra_da_invoice_com_identidade() -> NovaVersao {
    let mut v = regra_da_invoice();
    v.definicao.extrair.insert(
        "numero_da_invoice".into(),
        extracao(r"Invoice number\s+([A-Za-z0-9-]+)", Formato::Texto),
    );
    v.definicao.identidade = vec!["numero_da_invoice".into()];
    v
}

#[test]
fn a_mesma_invoice_por_dois_canais_e_possivel_duplicidade_e_nao_vincula_duas_vezes() {
    use sbf_inventario::caixa_de_entrada::{Canal, Estado, Filtro};
    let c = cenario();
    let id = c.criar("Nuvem Exemplo PRO", Natureza::Saida, 2_200, 20, None);
    c.instalar(regra_da_invoice_com_identidade());
    let r = c.com_regras().com_caixa_de_entrada(Canal::Cli, false);

    let primeira = c.rodar(&r, &c.solto("email.txt", INVOICE_POR_EMAIL));
    assert!(
        primeira.sugestoes.iter().any(|s| s.da_regra().is_some()),
        "a primeira sugere o vínculo"
    );
    let segunda = c.rodar(&r, &c.solto("telegram.txt", INVOICE_PELO_TELEGRAM));
    assert_ne!(primeira.sha256, segunda.sha256, "bytes diferentes");

    // Nenhum vínculo duplicado: sai um aviso que diz de quem é repetido.
    assert!(
        segunda.sugestoes.iter().all(|s| s.da_regra().is_none()),
        "{:?}",
        segunda.sugestoes
    );
    let curto: String = primeira.sha256.chars().take(8).collect();
    assert!(
        segunda
            .avisos
            .iter()
            .any(|a| a.contains("mesmo documento que") && a.contains(&curto)),
        "{:?}",
        segunda.avisos
    );
    // A Caixa marca a segunda como Possível duplicidade; a primeira não.
    let nome_da_primeira = "email.txt";
    let fila = sbf_inventario::caixa_de_entrada::listar(&c.conexao(), &Filtro::caixa()).unwrap();
    let estado_de = |sha: &str| {
        fila.iter()
            .find(|e| e.content_hash == sha)
            .map(|e| e.estado)
    };
    assert_eq!(
        estado_de(&segunda.sha256),
        Some(Estado::PossivelDuplicidade)
    );
    assert_ne!(
        estado_de(&primeira.sha256),
        Some(Estado::PossivelDuplicidade),
        "{nome_da_primeira}"
    );
    // E só um documento chega à ocorrência se o usuário confirmar a primeira.
    let mut plan = c.plan();
    confirmar(&mut plan, &primeira, so_vincular(&primeira));
    assert_eq!(
        plan.documentos(&ocorrencia_de(id, 2026, 11)).unwrap().len(),
        1
    );
}

#[test]
fn a_mesma_invoice_em_outro_padrao_nao_e_duplicidade() {
    use sbf_inventario::caixa_de_entrada::{Canal, Estado, Filtro};
    let c = cenario();
    c.criar("Nuvem Exemplo PRO", Natureza::Saida, 2_200, 20, None);
    c.instalar(regra_da_invoice_com_identidade());
    let mut recibo = regra_do_receipt();
    recibo.definicao.extrair.insert(
        "numero_da_invoice".into(),
        extracao(
            r"Receipt number\s+([A-Za-z0-9-]+?)(?:-R)?\s",
            Formato::Texto,
        ),
    );
    recibo.definicao.identidade = vec!["numero_da_invoice".into()];
    c.instalar(recibo);
    let r = c.com_regras().com_caixa_de_entrada(Canal::Cli, false);
    c.rodar(&r, &c.solto("invoice.txt", INVOICE_POR_EMAIL));
    let recibo = c.rodar(&r, &c.solto("receipt.txt", RECEIPT_DA_NUVEM));
    assert!(
        recibo.sugestoes.iter().any(|s| s.da_regra().is_some()),
        "{:?} {:?}",
        recibo.sugestoes,
        recibo.avisos
    );
    let fila = sbf_inventario::caixa_de_entrada::listar(&c.conexao(), &Filtro::caixa()).unwrap();
    assert!(
        fila.iter().all(|e| e.estado != Estado::PossivelDuplicidade),
        "{fila:?}"
    );
}

#[test]
fn identidade_repetida_chega_ao_estado_da_caixa_sem_depender_do_sha() {
    use sbf_reconhecimento::caixa_de_entrada::{estado_do_resultado, leitura_do_resultado};
    let c = cenario();
    c.criar("Nuvem Exemplo PRO", Natureza::Saida, 2_200, 20, None);
    c.instalar(regra_da_invoice_com_identidade());
    let r = c.com_regras();
    let primeira = c.rodar(&r, &c.solto("email.txt", INVOICE_POR_EMAIL));
    let segunda = c.rodar(&r, &c.solto("telegram.txt", INVOICE_PELO_TELEGRAM));
    assert_ne!(
        estado_do_resultado(&primeira, false),
        sbf_inventario::caixa_de_entrada::Estado::PossivelDuplicidade
    );
    assert_eq!(
        estado_do_resultado(&segunda, false),
        sbf_inventario::caixa_de_entrada::Estado::PossivelDuplicidade
    );
    assert_eq!(
        leitura_do_resultado(&segunda, false).estado,
        sbf_inventario::caixa_de_entrada::Estado::PossivelDuplicidade
    );
}
