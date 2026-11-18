//! Regras de reconhecimento sobre registros da Pluggy que identificam o favorecido de um Pix
//! e sugerem a conciliação de um compromisso mensal, combinando vários débitos (spec 13,
//! D4, D13). Cofre temporário e dados fictícios ("Fulana de Tal", documento
//! "000.000.000-00"); nenhuma IA.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use rusqlite::Connection;
use sbf_inventario::banco::Inventario;
use sbf_planejamento::{
    Alteracao, Autor, Banco, Centavos, Certeza, Competencia, Conciliacao, Data, Filtro, Fonte,
    Frequencia, IdOcorrencia, MovimentoDoExtrato, Natureza, NovoRecorrente, Periodo, Planejamento,
    PlanejamentoSqlite, Prazo, SemSaldos, Versao,
};
use sbf_planilha::movimentos::MovimentosDaPluggy;
use sbf_reconhecimento::favorecido::{self, rotulo_da_sugestao};
use sbf_reconhecimento::regras::{ARQUIVO_DAS_REGRAS, entrada_do_registro};
use sbf_regras::catalogo::{Catalogo, EstadoDaProposta, NovaVersao};
use sbf_regras::definicao::{
    Condicao, Definicao, Extracao, FonteDaExtracao, Formato, ModeloDeProposta,
};
use sbf_regras::motor::Motor;
use sbf_regras::{Autor as AutorDaRegra, Decisao, Estado, Quem, Retorno};

const CONTA: &str = "Banco Exemplo 000000-1";
const DOC: &str = "000.000.000-00";

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

fn comp(ano: i32, mes: u8) -> Competencia {
    Competencia { ano, mes }
}

fn dia(m: u8, d: u8) -> Data {
    Data::nova(2026, m, d).unwrap()
}

fn novembro() -> Periodo {
    Periodo::MesCivil(comp(2026, 11))
}

fn hoje() -> Data {
    dia(11, 18)
}

fn ocorrencia(recorrente: &str) -> IdOcorrencia {
    IdOcorrencia {
        recorrente: recorrente.to_owned(),
        competencia: comp(2026, 11),
        parcela: None,
    }
}

impl Cenario {
    fn conexao(&self) -> Connection {
        Connection::open(self.cofre.join("inventario.sqlite")).expect("abrir")
    }

    fn fonte(&self) -> MovimentosDaPluggy {
        MovimentosDaPluggy::do_cofre(&self.cofre)
    }

    fn plan(&self) -> PlanejamentoSqlite {
        let banco = Banco::abrir(self.cofre.join("planejamento.sqlite")).expect("banco");
        PlanejamentoSqlite::novo(banco, Box::new(SemSaldos)).com_movimentos(Box::new(self.fonte()))
    }

    fn catalogo(&self) -> Catalogo {
        Catalogo::abrir(&self.cofre.join(ARQUIVO_DAS_REGRAS)).expect("catalogo")
    }

    /// Um recorrente mensal fictício; devolve o id.
    fn criar(&self, nome: &str, valor: i64, dia: u8, conta: Option<&str>) -> String {
        let mut plan = self.plan();
        plan.aplicar(
            Alteracao::CriarRecorrente {
                recorrente: NovoRecorrente {
                    nome: nome.into(),
                    grupo: "Casa".into(),
                    subgrupo: None,
                    posicao: 0,
                    natureza: Natureza::Saida,
                    prazo: Prazo::SemFim,
                    primeira_versao: Versao {
                        id: String::new(),
                        desde: comp(2026, 11),
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
                    apelido: "CX-0001".into(),
                    conta_do_extrato: Some(CONTA.into()),
                },
                Autor::Usuario,
            )
            .expect("mapear");
    }

    /// Um débito no extrato do inventário (a Pluggy), com o favorecido nas colunas à parte
    /// (a descrição é só "PIX ENVIADO", como na CAIXA).
    fn pix(&self, id: &str, data: &str, centavos: i64, nome: &str, doc: &str, conta: &str) {
        let conn = self.conexao();
        conn.execute(
            "INSERT OR IGNORE INTO fonte (id, tipo, descricao)
             VALUES ('conexao:pluggy', 'conexao', 'x')",
            [],
        )
        .expect("fonte");
        conn.execute(
            "INSERT INTO registro (external_ref, fonte_id, tipo, data, data_tipo, valor_centavos,
                                   descricao_original, conta_provavel, extraido_por,
                                   contraparte, contraparte_doc)
             VALUES (?1, 'conexao:pluggy', 'debito', ?2, 'pagamento', ?3, 'PIX ENVIADO', ?4,
                     'pluggy@1', ?5, ?6)",
            rusqlite::params![id, data, -centavos, conta, nome, doc],
        )
        .expect("registro");
    }

    fn instalar(&self, nova: NovaVersao) {
        let mut c = self.catalogo();
        let v = c.salvar_rascunho(nova).expect("rascunho");
        c.ativar(&v.regra, v.versao, &Quem::Usuario, None)
            .expect("ativar");
    }

    /// 5 propostas aceitas seguidas e a consolidação pelo usuário (D4).
    fn consolidar(&self, regra: &str) {
        let mut c = self.catalogo();
        let motor = Motor::das_vigentes(&c, None).expect("motor");
        for i in 0..5 {
            let m = movimento_de_exemplo(&format!("pluggy:txn_ex{i}"));
            let e = entrada_do_registro(&m);
            let Decisao::Reconhecido(rec) = motor.ler(&e).decisao else {
                panic!("a regra devia reconhecer");
            };
            c.registrar_proposta(&rec, &e.id).expect("proposta");
            c.registrar_retorno(&rec.chave, Retorno::Aceito, &Quem::Usuario, "")
                .expect("retorno");
        }
        c.consolidar(regra, 1, &Quem::Usuario).expect("consolidar");
        assert_eq!(c.versao(regra, 1).expect("v").estado, Estado::Consolidada);
    }

    /// `buscar` do jeito que a janela chama.
    fn buscar(&self) -> usize {
        favorecido::buscar(
            &mut self.plan(),
            &mut self.catalogo(),
            None,
            &self.fonte(),
            &novembro(),
            hoje(),
        )
        .expect("buscar")
    }

    fn sugerir(&self) -> Vec<favorecido::SugestaoDoFavorecido> {
        let c = self.catalogo();
        let motor = Motor::das_vigentes(&c, None).expect("motor");
        favorecido::sugerir(
            &self.plan(),
            &motor,
            &self.fonte(),
            Some(&c),
            &novembro(),
            hoje(),
        )
        .expect("sugerir")
    }
}

fn movimento_de_exemplo(id: &str) -> MovimentoDoExtrato {
    MovimentoDoExtrato {
        movimento: id.into(),
        data: dia(10, 5),
        valor: Centavos(-30_000),
        conta_do_extrato: CONTA.into(),
        descricao: "PIX ENVIADO".into(),
        tipo: "debito".into(),
        contraparte: Some("Fulana de Tal".into()),
        contraparte_doc: Some(DOC.into()),
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

/// A regra da diarista: reconhece o Pix pelo documento do favorecido e propõe conciliar a
/// ocorrência do compromisso "Diarista".
fn regra_da_diarista(condicao: Condicao, janela: u32) -> NovaVersao {
    let mut d = Definicao::simples("pix-da-diarista", "x");
    d.aplica_a = vec![sbf_regras::TipoDeEntrada::Registro];
    d.detectar.todos = vec![condicao];
    d.extrair.insert(
        "favorecido".into(),
        Extracao {
            regex: "(.+)".into(),
            grupo: 1,
            formato: Formato::Texto,
            fonte: FonteDaExtracao::Contraparte,
        },
    );
    d.propor = vec![ModeloDeProposta::ConciliarOcorrencia {
        janela_dias: janela,
        compromisso: Some("Diarista".into()),
    }];
    nova("pix-da-diarista", d)
}

fn pelo_documento() -> Condicao {
    Condicao::ContraparteDoc(vec![DOC.into()])
}

/// Diarista de R$ 300,00, dia 25, na conta mapeada, com a regra ativa.
fn com_diarista() -> (Cenario, String) {
    let c = cenario();
    let id = c.criar("Diarista", 30_000, 25, Some("CX-0001"));
    c.mapear_conta();
    c.instalar(regra_da_diarista(pelo_documento(), 5));
    (c, id)
}

fn linha(plan: &PlanejamentoSqlite, nome: &str) -> sbf_planejamento::Ocorrencia {
    plan.controle(&novembro(), Filtro::Todos, None, hoje())
        .unwrap()
        .linhas
        .into_iter()
        .find(|o| o.nome == nome)
        .unwrap()
}

// ----------------------------------------------------------------- a sugestão

#[test]
fn um_debito_com_o_valor_previsto_vira_sugestao_de_um() {
    let (c, id) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-12",
        30_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    // Outro Pix do mesmo favorecido, de outro valor, não atrapalha: o exato basta.
    c.pix(
        "pluggy:txn_2",
        "2026-11-13",
        4_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );

    let s = c.sugerir();

    assert_eq!(s.len(), 1);
    let s = &s[0];
    assert_eq!(s.ocorrencia, ocorrencia(&id));
    assert_eq!(s.debitos.len(), 1);
    assert_eq!(s.debitos[0].movimento.movimento, "pluggy:txn_1");
    assert!(s.exata);
    assert_eq!(s.aviso, None);
    assert_eq!(s.soma, Centavos(30_000));
    assert_eq!(s.favorecido.as_deref(), Some("Fulana de Tal"));
    assert_eq!(s.rotulo, "Pix para Fulana de Tal · o débito é o previsto");
    // Regra reconhece o favorecido, valor exato e conta mapeada.
    assert_eq!(s.provas, 3);
    assert!(!s.automatica, "regra ativa só propõe");
}

#[test]
fn dois_debitos_que_somam_o_previsto_viram_uma_sugestao_combinada() {
    let (c, _) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        15_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    // O mesmo documento sem pontuação reconhece do mesmo jeito.
    c.pix(
        "pluggy:txn_2",
        "2026-11-19",
        15_000,
        "FULANA DE TAL",
        "00000000000",
        CONTA,
    );

    let s = c.sugerir();

    assert_eq!(s.len(), 1);
    assert_eq!(s[0].debitos.len(), 2);
    assert!(s[0].exata);
    assert_eq!(s[0].aviso, None);
    assert_eq!(
        s[0].rotulo,
        "Pix para Fulana de Tal · 2 débitos somam o previsto"
    );
}

#[test]
fn o_menor_subconjunto_exato_vale_quando_a_soma_de_todos_passa() {
    let (c, _) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        10_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    c.pix(
        "pluggy:txn_2",
        "2026-11-08",
        20_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    c.pix(
        "pluggy:txn_3",
        "2026-11-12",
        7_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );

    let s = c.sugerir();

    let ids: Vec<_> = s[0]
        .debitos
        .iter()
        .map(|d| d.movimento.movimento.as_str())
        .collect();
    assert_eq!(ids, vec!["pluggy:txn_1", "pluggy:txn_2"]);
    assert!(s[0].exata);
}

#[test]
fn soma_que_nao_bate_sugere_todos_com_aviso() {
    let (c, _) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        10_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    c.pix(
        "pluggy:txn_2",
        "2026-11-08",
        12_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );

    let s = c.sugerir();

    assert_eq!(s[0].debitos.len(), 2);
    assert!(!s[0].exata);
    assert_eq!(
        s[0].aviso.as_deref(),
        Some("⚠ soma R$ 220,00 difere do previsto R$ 300,00")
    );
    assert_eq!(
        s[0].rotulo,
        "Pix para Fulana de Tal · ⚠ soma R$ 220,00 difere do previsto R$ 300,00"
    );
    assert_eq!(s[0].provas, 2, "documento e conta; o valor não é exato");
    assert!(!s[0].automatica);
}

#[test]
fn debito_ja_conciliado_a_outra_ocorrencia_nao_entra() {
    let (c, _) = com_diarista();
    let outra = c.criar("Material", 15_000, 10, Some("CX-0001"));
    for (id, d) in [
        ("a", "2026-11-05"),
        ("b", "2026-11-08"),
        ("c", "2026-11-12"),
    ] {
        c.pix(
            &format!("pluggy:txn_{id}"),
            d,
            15_000,
            "Fulana de Tal",
            DOC,
            CONTA,
        );
    }
    c.plan()
        .aplicar(
            Alteracao::Conciliar {
                conciliacao: Conciliacao {
                    ocorrencia: ocorrencia(&outra),
                    movimento: "pluggy:txn_a".into(),
                    valor: Centavos(15_000),
                    data: dia(11, 5),
                    confirmada: true,
                },
            },
            Autor::Usuario,
        )
        .unwrap();

    let s = c.sugerir();

    let ids: Vec<_> = s[0]
        .debitos
        .iter()
        .map(|d| d.movimento.movimento.as_str())
        .collect();
    assert_eq!(ids, vec!["pluggy:txn_b", "pluggy:txn_c"]);
    assert!(s[0].exata);
}

#[test]
fn regra_em_rascunho_ou_desativada_nao_sugere() {
    let c = cenario();
    c.criar("Diarista", 30_000, 25, Some("CX-0001"));
    c.mapear_conta();
    c.pix(
        "pluggy:txn_1",
        "2026-11-12",
        30_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    // Só salva: nasce rascunho.
    c.catalogo()
        .salvar_rascunho(regra_da_diarista(pelo_documento(), 5))
        .unwrap();
    assert!(c.sugerir().is_empty());
    assert_eq!(c.buscar(), 0);

    // Ativa sugere; desativada, não.
    c.catalogo()
        .ativar("pix-da-diarista", 1, &Quem::Usuario, None)
        .unwrap();
    assert_eq!(c.sugerir().len(), 1);
    c.catalogo()
        .desativar("pix-da-diarista", &Quem::Usuario)
        .unwrap();
    assert!(c.sugerir().is_empty());
}

#[test]
fn outro_favorecido_ou_outro_compromisso_nao_casa() {
    let (c, _) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-12",
        30_000,
        "Fulano de Tal",
        "111.111.111-11",
        CONTA,
    );
    assert!(c.sugerir().is_empty());

    // Um compromisso que não tem o nome da proposta não recebe a sugestão.
    let c2 = cenario();
    c2.criar("Jardineiro", 30_000, 25, Some("CX-0001"));
    c2.mapear_conta();
    c2.instalar(regra_da_diarista(pelo_documento(), 5));
    c2.pix(
        "pluggy:txn_1",
        "2026-11-12",
        30_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    assert!(c2.sugerir().is_empty());
}

#[test]
fn a_condicao_por_nome_tambem_serve() {
    let c = cenario();
    c.criar("Diarista", 30_000, 25, Some("CX-0001"));
    c.mapear_conta();
    c.instalar(regra_da_diarista(
        Condicao::Contraparte("^fulana de tal$".into()),
        5,
    ));
    c.pix(
        "pluggy:txn_1",
        "2026-11-12",
        30_000,
        "Fulana de Tal",
        "",
        CONTA,
    );
    assert_eq!(c.sugerir().len(), 1);
}

#[test]
fn a_janela_e_a_competencia_inteira_mais_a_folga_em_volta_do_vencimento() {
    let c = cenario();
    // Vence no dia 29: a folga de 5 dias passa para dezembro.
    c.criar("Diarista", 20_000, 29, Some("CX-0001"));
    c.mapear_conta();
    c.instalar(regra_da_diarista(pelo_documento(), 5));
    // No início do mês (dia 2): 27 dias antes do vencimento, mas dentro da competência.
    c.pix(
        "pluggy:txn_inicio",
        "2026-11-02",
        5_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    // Dentro da folga, já em dezembro.
    c.pix(
        "pluggy:txn_folga",
        "2026-12-03",
        5_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    // Fora da competência e fora da folga (7 dias depois).
    c.pix(
        "pluggy:txn_fora",
        "2026-12-06",
        10_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    // Outubro: outra competência e longe.
    c.pix(
        "pluggy:txn_outubro",
        "2026-10-15",
        10_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );

    let s = c.sugerir();

    let mut ids: Vec<_> = s[0]
        .debitos
        .iter()
        .map(|d| d.movimento.movimento.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["pluggy:txn_folga", "pluggy:txn_inicio"]);
}

#[test]
fn credito_nunca_entra() {
    let (c, _) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-12",
        -30_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    assert!(c.sugerir().is_empty());
}

// ----------------------------------------------------- gravar, aplicar e retorno

#[test]
fn buscar_grava_so_sugestoes_e_nada_muda_sem_clique() {
    let (c, id) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        15_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    c.pix(
        "pluggy:txn_2",
        "2026-11-19",
        15_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    let antes = linha(&c.plan(), "Diarista");
    assert!(!antes.sugestao_de_conciliacao);

    assert_eq!(c.buscar(), 1);

    let plan = c.plan();
    let depois = linha(&plan, "Diarista");
    // O Controle mensal e o MCP leem este mesmo campo.
    assert!(depois.sugestao_de_conciliacao);
    assert_eq!(depois.conciliado, None);
    assert_eq!(depois.estado, antes.estado);
    let gravadas = plan.conciliacoes_da_ocorrencia(&ocorrencia(&id)).unwrap();
    assert_eq!(gravadas.len(), 2);
    assert!(gravadas.iter().all(|g| !g.conciliacao.confirmada));
    assert!(gravadas.iter().all(|g| !g.automatica));
    let json = serde_json::to_string(&depois).unwrap();
    assert!(json.contains("\"sugestao_de_conciliacao\":true"), "{json}");
    // As propostas ficam abertas; ninguém decidiu nada.
    let propostas = c.catalogo().propostas(None).unwrap();
    assert_eq!(propostas.len(), 2);
    assert!(
        propostas
            .iter()
            .all(|p| p.estado == EstadoDaProposta::Aberta)
    );
    // Buscar de novo não duplica.
    assert_eq!(c.buscar(), 0);
    assert_eq!(c.catalogo().propostas(None).unwrap().len(), 2);
    assert_eq!(
        c.plan()
            .conciliacoes_da_ocorrencia(&ocorrencia(&id))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn aplicar_grava_uma_conciliacao_confirmada_por_debito_e_o_conciliado_soma() {
    let (c, id) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        15_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    c.pix(
        "pluggy:txn_2",
        "2026-11-19",
        15_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    c.buscar();

    let mut plan = c.plan();
    let mut cat = c.catalogo();
    let feitas = favorecido::confirmar(&mut plan, Some(&mut cat), &ocorrencia(&id)).unwrap();

    assert_eq!(feitas.len(), 2);
    assert!(feitas.iter().all(|f| f.confirmada));
    let plan = c.plan();
    let o = linha(&plan, "Diarista");
    assert_eq!(o.conciliado, Some(Centavos(30_000)), "Conciliado soma");
    assert!(!o.sugestao_de_conciliacao);
    let gravadas = plan.conciliacoes_da_ocorrencia(&ocorrencia(&id)).unwrap();
    assert_eq!(
        gravadas.iter().filter(|g| g.conciliacao.confirmada).count(),
        2
    );
    // O autor é o clique do usuário, não a regra.
    assert!(gravadas.iter().all(|g| !g.automatica));
    // Retorno: cada proposta foi aceita.
    let propostas = c.catalogo().propostas(None).unwrap();
    assert!(
        propostas
            .iter()
            .all(|p| p.estado == EstadoDaProposta::Aceita)
    );
    assert_eq!(
        c.catalogo()
            .versao("pix-da-diarista", 1)
            .unwrap()
            .aceitas_seguidas,
        2
    );
    // Uma segunda busca não sugere os mesmos débitos de novo.
    assert_eq!(c.buscar(), 0);
}

#[test]
fn ignorar_rejeita_as_propostas_desfaz_as_sugestoes_e_nao_volta() {
    let (c, id) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        15_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    c.pix(
        "pluggy:txn_2",
        "2026-11-19",
        15_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    c.buscar();

    let n = favorecido::ignorar(&mut c.plan(), Some(&mut c.catalogo()), &ocorrencia(&id)).unwrap();

    assert_eq!(n, 2);
    let plan = c.plan();
    let o = linha(&plan, "Diarista");
    assert!(!o.sugestao_de_conciliacao);
    assert_eq!(o.conciliado, None);
    let propostas = c.catalogo().propostas(None).unwrap();
    assert!(
        propostas
            .iter()
            .all(|p| p.estado == EstadoDaProposta::Recusada)
    );
    assert_eq!(c.buscar(), 0, "o que o usuário ignorou não volta");
    assert!(c.sugerir().is_empty());
}

#[test]
fn um_debito_novo_depois_do_ignorar_ainda_sugere_sem_os_antigos() {
    let (c, id) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        10_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    c.buscar();
    favorecido::ignorar(&mut c.plan(), Some(&mut c.catalogo()), &ocorrencia(&id)).unwrap();
    c.pix(
        "pluggy:txn_2",
        "2026-11-20",
        30_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );

    let s = c.sugerir();

    assert_eq!(s.len(), 1);
    assert_eq!(s[0].debitos.len(), 1);
    assert_eq!(s[0].debitos[0].movimento.movimento, "pluggy:txn_2");
}

#[test]
fn ocorrencia_que_ja_tem_sugestao_ou_conciliacao_nao_e_refeita() {
    let (c, id) = com_diarista();
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        30_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    assert_eq!(c.buscar(), 1);
    c.pix(
        "pluggy:txn_2",
        "2026-11-06",
        30_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    assert!(c.sugerir().is_empty(), "já há sugestão pendente");
    favorecido::confirmar(&mut c.plan(), Some(&mut c.catalogo()), &ocorrencia(&id)).unwrap();
    assert!(c.sugerir().is_empty(), "já está conciliada");
}

#[test]
fn confirmar_sem_sugestao_pendente_nao_inventa_nada() {
    let (c, id) = com_diarista();
    let feitas = favorecido::confirmar(&mut c.plan(), None, &ocorrencia(&id)).unwrap();
    assert!(feitas.is_empty());
}

// --------------------------------------------------------- consolidada, automática

#[test]
fn regra_consolidada_com_soma_exata_e_conta_concilia_sozinha_com_desfazer() {
    let (c, id) = com_diarista();
    c.consolidar("pix-da-diarista");
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        15_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    c.pix(
        "pluggy:txn_2",
        "2026-11-19",
        15_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );

    let s = c.sugerir();
    assert!(s[0].automatica);
    assert_eq!(s[0].provas, 3);

    assert_eq!(c.buscar(), 1);

    let plan = c.plan();
    assert_eq!(linha(&plan, "Diarista").conciliado, Some(Centavos(30_000)));
    let gravadas = plan.conciliacoes_da_ocorrencia(&ocorrencia(&id)).unwrap();
    assert!(
        gravadas
            .iter()
            .all(|g| g.conciliacao.confirmada && g.automatica)
    );
    // As propostas esperam o "Desfazer" para virar retorno (nunca passaram por um aceito).
    let cat = c.catalogo();
    assert!(
        cat.propostas(None)
            .unwrap()
            .iter()
            .filter(|p| !p.entrada.starts_with("pluggy:txn_ex"))
            .all(|p| p.estado == EstadoDaProposta::Aberta)
    );
    let chaves = favorecido::chaves_abertas(&cat, "pluggy:txn_1");
    assert_eq!(chaves.len(), 1);

    // O Desfazer registra aceito e desfeito: zera a sequência e rebaixa a regra a ativa.
    let mut cat = c.catalogo();
    favorecido::desfeito(&mut cat, "pluggy:txn_1");
    assert_eq!(
        cat.versao("pix-da-diarista", 1).unwrap().estado,
        Estado::Ativa
    );
    assert!(favorecido::chaves_abertas(&cat, "pluggy:txn_1").is_empty());
}

#[test]
fn consolidada_mas_sem_soma_exata_ou_sem_conta_so_propoe() {
    // Soma que não bate.
    let (c, _) = com_diarista();
    c.consolidar("pix-da-diarista");
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        10_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    assert!(!c.sugerir()[0].automatica);
    assert_eq!(c.buscar(), 1);
    let plan = c.plan();
    assert_eq!(linha(&plan, "Diarista").conciliado, None);

    // Valor exato, mas o débito saiu de outra conta.
    let (c2, _) = com_diarista();
    c2.consolidar("pix-da-diarista");
    c2.pix(
        "pluggy:txn_1",
        "2026-11-05",
        30_000,
        "Fulana de Tal",
        DOC,
        "Outro Banco 999",
    );
    let s = c2.sugerir();
    assert_eq!(s[0].provas, 2);
    assert!(!s[0].automatica);
}

#[test]
fn par_desfeito_antes_nao_e_refeito_sozinho() {
    let (c, id) = com_diarista();
    c.consolidar("pix-da-diarista");
    c.pix(
        "pluggy:txn_1",
        "2026-11-05",
        30_000,
        "Fulana de Tal",
        DOC,
        CONTA,
    );
    // O usuário já conciliou e desfez este par à mão.
    let mut plan = c.plan();
    let conc = Conciliacao {
        ocorrencia: ocorrencia(&id),
        movimento: "pluggy:txn_1".into(),
        valor: Centavos(30_000),
        data: dia(11, 5),
        confirmada: true,
    };
    plan.aplicar(Alteracao::Conciliar { conciliacao: conc }, Autor::Usuario)
        .unwrap();
    plan.aplicar(
        Alteracao::Desconciliar {
            ocorrencia: ocorrencia(&id),
            movimento: "pluggy:txn_1".into(),
        },
        Autor::Usuario,
    )
    .unwrap();

    let s = c.sugerir();

    assert_eq!(s.len(), 1);
    assert!(!s[0].automatica);
}

// ------------------------------------------------------------------- o rótulo

#[test]
fn o_rotulo_diz_o_que_a_soma_faz() {
    assert_eq!(
        rotulo_da_sugestao(Some("Fulana de Tal"), 2, 30_000, Some(30_000)),
        "Pix para Fulana de Tal · 2 débitos somam o previsto"
    );
    assert_eq!(
        rotulo_da_sugestao(Some("Fulana de Tal"), 1, 30_000, Some(30_000)),
        "Pix para Fulana de Tal · o débito é o previsto"
    );
    assert_eq!(
        rotulo_da_sugestao(None, 3, 1_000, Some(30_000)),
        "Pix · ⚠ soma R$ 10,00 difere do previsto R$ 300,00"
    );
    assert_eq!(
        rotulo_da_sugestao(Some("Fulana de Tal"), 2, 30_000, None),
        "Pix para Fulana de Tal · 2 débitos, sem valor previsto para comparar"
    );
}
