//! A agenda, os parâmetros do roteiro, a suspensão por entrada recusada e a baixa por conteúdo
//! ligadas ao banco e ao serviço (spec 18). Tudo fictício (`portal.exemplo.test`), banco
//! temporário, navegador falso: nunca o `roteiros.sqlite` real.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use bancada_navegador::falso::{Elemento, NavegadorFalso};
use bancada_navegador::pagina::Pagina;
use comum::{PausaFalsa, Senhas, clicavel, inventario};
use rusqlite::Connection;
use sbf_roteiros::agenda::instante_local as l;
use sbf_roteiros::banco::{
    AutorEscrito, BancoDeRoteiros, Confirmacao, EstadoDaExecucao, ResultadoDoTeste, VersaoEscrita,
};
use sbf_roteiros::codigo_email::Relogio;
use sbf_roteiros::escrito::OpcoesEscrito;
use sbf_roteiros::execucao_escrita::InicioDaExecucao;
use sbf_roteiros::executor::{Fim, Opcoes};
use sbf_roteiros::manifesto::Manifesto;
use sbf_roteiros::parametros::ValorBruto;
use sbf_roteiros::servico::{
    Ambiente, PedidoDeExecucao, Servicos, executar_escrito_em_com, executar_escritos_agendados_com,
    resolver_pedido,
};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

// ------------------------------------------------------------------------------------------
// Apoio
// ------------------------------------------------------------------------------------------

const PARAMETROS: &str = r##""parametros":[
    {"nome":"de","tipo":"mes","padrao":"0"},
    {"nome":"ate","tipo":"mes","padrao":"0"},
    {"nome":"forcar","tipo":"booleano","padrao":false}]"##;

/// O manifesto com `extra` (campos a mais, já com vírgula inicial se preciso).
fn manifesto_json(nome: &str, extra: &str) -> String {
    format!(
        r##"{{"nome":"{nome}","descricao":"fictício","url_inicial":"https://portal.exemplo.test/entrar",
        "hosts":["portal.exemplo.test"],"rotulo_senha":null,"rotulo_cpf":null,
        "mfa":{{"tipo":"nenhum"}},"movimenta_dinheiro":false,"classe":"consulta"{extra}}}"##
    )
}

fn manifesto(nome: &str, extra: &str) -> Manifesto {
    Manifesto::de_json(&manifesto_json(nome, extra)).unwrap()
}

fn invalido(extra: &str) -> String {
    match Manifesto::de_json(&manifesto_json("x", extra)) {
        Ok(_) => panic!("devia recusar: {extra}"),
        Err(e) => e.to_string(),
    }
}

fn ambiente(dir: &TempDir) -> (Ambiente, Connection) {
    let (cofre, conn) = inventario(dir.path());
    let mut a = Ambiente::novo(&dir.path().join("dados"), &cofre);
    a.opcoes = Opcoes {
        espera: Duration::from_millis(60),
        download: Duration::from_secs(1),
        intervalo: Duration::from_millis(5),
    };
    a.prazo_da_pausa = Duration::from_secs(5);
    a.sem_janela = true;
    (a, conn)
}

fn passou() -> ResultadoDoTeste {
    ResultadoDoTeste {
        passou: true,
        resumo: "verde".into(),
    }
}

fn ativar(amb: &Ambiente, m: &Manifesto, fonte: &str) -> VersaoEscrita {
    let banco = amb.banco().unwrap();
    let n = banco
        .importar_escrito(m, fonte, &AutorEscrito::Usuario, "teste")
        .unwrap()
        .numero;
    banco.marcar_testado(&m.nome, n, &passou()).unwrap();
    let digitado = Confirmacao::texto_de_ativar(&m.nome, n);
    let c = Confirmacao::do_terminal(true, &digitado, &digitado).unwrap();
    banco.ativar(&m.nome, n, &c).unwrap();
    banco.versao_ativa_escrita(&m.nome).unwrap().unwrap()
}

/// Troca o instante em que a versão foi ativada (o histórico é imutável: só num banco de
/// teste se tira o gatilho).
fn fixar_ativacao(amb: &Ambiente, unix: i64) {
    let c = Connection::open(amb.arquivo_do_banco()).unwrap();
    c.execute_batch("DROP TRIGGER roteiro_escrito_evento_imutavel;")
        .unwrap();
    c.execute(
        "UPDATE roteiro_escrito_evento
            SET quando = strftime('%Y-%m-%dT%H:%M:%fZ', ?1, 'unixepoch') WHERE para = 'ativo'",
        [unix],
    )
    .unwrap();
}

/// Registra que a entrada rodou e foi iniciada em `inicio` (unix).
fn rodou(amb: &Ambiente, v: &VersaoEscrita, entrada: usize, inicio: i64) -> i64 {
    let banco = amb.banco().unwrap();
    let id = banco
        .iniciar_execucao_escrita_com(
            &v.roteiro,
            v.numero,
            "agenda",
            &InicioDaExecucao {
                entrada: Some(entrada),
                previsto_unix: Some(inicio),
                parametros_json: None,
            },
        )
        .unwrap();
    banco
        .finalizar_escrita(id, EstadoDaExecucao::Concluida, None, 0, 0)
        .unwrap();
    Connection::open(amb.arquivo_do_banco())
        .unwrap()
        .execute(
            "UPDATE execucao_escrita SET iniciada_unix = ?2 WHERE id = ?1",
            rusqlite::params![id, inicio],
        )
        .unwrap();
    id
}

struct RelogioFixo(i64);

impl Relogio for RelogioFixo {
    fn agora(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(u64::try_from(self.0).unwrap())
    }
    fn esperar(&self, _d: Duration) {}
}

/// O navegador falso com o painel e um botão que baixa um PDF fictício.
fn painel(conteudo: &[u8]) -> NavegadorFalso {
    let mut n = NavegadorFalso::default();
    n.rotas
        .insert("https://portal.exemplo.test/painel".into(), "painel".into());
    n.pagina_nomeada(
        "painel",
        Pagina {
            url: "https://portal.exemplo.test/painel".into(),
            titulo: "Painel".into(),
            texto: "Seus contracheques".into(),
            elementos: vec![clicavel("#baixar", "Baixar contracheque")],
            ..Pagina::default()
        },
    );
    let mut baixar = Elemento::com_texto("Baixar contracheque");
    baixar.baixa = Some(("contracheque.pdf".into(), conteudo.to_vec()));
    n.elementos.insert("#baixar".into(), baixar);
    n.atual = "painel".into();
    n
}

/// Roda a versão, com o relógio em `agora`, e devolve o relatório e o id da execução.
fn executar(
    amb: &Ambiente,
    v: &VersaoEscrita,
    n: &mut NavegadorFalso,
    pedido: &PedidoDeExecucao,
    agora: i64,
) -> (sbf_roteiros::servico::RelatorioEscrito, i64) {
    let banco = amb.banco().unwrap();
    let resolvida = resolver_pedido(&v.manifesto, pedido, agora).unwrap();
    let id = banco
        .iniciar_execucao_escrita_com(&v.roteiro, v.numero, "janela", &resolvida.inicio)
        .unwrap();
    let relogio = RelogioFixo(agora);
    let pausa = PausaFalsa::com(&[]);
    let opcoes = OpcoesEscrito::default();
    let rel = executar_escrito_em_com(
        amb,
        n,
        v,
        id,
        &Servicos {
            senhas: &Senhas,
            caixa: None,
            relogio: &relogio,
            pausa: Some(&pausa),
            opcoes: &opcoes,
        },
        &AtomicBool::new(false),
        &resolvida,
    )
    .unwrap();
    (rel, id)
}

/// 2026-11-25T01:00:00Z: 22h de 24/11 em UTC−3.
const AGORA: i64 = 1_795_568_400;

// ------------------------------------------------------------------------------------------
// Manifesto: agenda e parâmetros
// ------------------------------------------------------------------------------------------

#[test]
fn o_manifesto_com_agenda_e_parametros_le_e_volta_igual() {
    let extra = format!(
        r##","agenda":[{{"horarios":["04:00","12:00","20:00"],"dias_do_mes":[20,21,30,1],
            "parametros":{{"de":"-1","ate":"0"}}}},{{"horarios":["09:30"]}}],{PARAMETROS}"##
    );
    let m = manifesto("contracheque-exemplo", &extra);
    assert_eq!(m.agenda.len(), 2);
    assert_eq!(m.agenda[0].dias_do_mes, Some(vec![20, 21, 30, 1]));
    assert_eq!(m.agenda[1].dias_do_mes, None);
    assert_eq!(m.parametros.len(), 3);
    // ida e volta pelo JSON canônico
    let de_novo = Manifesto::de_json(&m.json_canonico().unwrap()).unwrap();
    assert_eq!(m, de_novo);
    assert_eq!(
        m.checksum_com("x").unwrap(),
        de_novo.checksum_com("x").unwrap()
    );
}

#[test]
fn o_manifesto_antigo_nao_muda_de_checksum() {
    // Sem `agenda` nem `parametros` o JSON canônico nem os cita: versões já importadas
    // continuam com o mesmo checksum.
    let m = manifesto("contracheque-exemplo", r#","agenda_horas":168"#);
    let json = m.json_canonico().unwrap();
    assert!(!json.contains("agenda\""), "{json}");
    assert!(!json.contains("parametros"), "{json}");
    assert!(json.ends_with(r#""agenda_horas":168}"#), "{json}");
}

#[test]
fn agenda_e_agenda_horas_juntas_sao_invalidas() {
    let e = invalido(r#","agenda_horas":24,"agenda":[{"horarios":["04:00"]}]"#);
    assert!(e.contains("agenda_horas"), "{e}");
    // cada uma sozinha vale
    manifesto("x", r#","agenda_horas":24"#);
    manifesto("x", r#","agenda":[{"horarios":["04:00"]}]"#);
}

#[test]
fn agenda_aceita_ate_16_entradas() {
    let entrada = r#"{"horarios":["04:00"]}"#;
    let com = |n: usize| format!(r#","agenda":[{}]"#, vec![entrada; n].join(","));
    manifesto("x", &com(16));
    let e = invalido(&com(17));
    assert!(e.contains("16"), "{e}");
}

#[test]
fn horarios_validam_formato_quantidade_e_repeticao() {
    for ok in [r#"["00:00"]"#, r#"["23:59"]"#, r#"["04:00","04:01"]"#] {
        manifesto("x", &format!(r#","agenda":[{{"horarios":{ok}}}]"#));
    }
    let vinte_e_quatro: Vec<String> = (0..24).map(|h| format!("\"{h:02}:00\"")).collect();
    manifesto(
        "x",
        &format!(
            r#","agenda":[{{"horarios":[{}]}}]"#,
            vinte_e_quatro.join(",")
        ),
    );
    let vinte_e_cinco: Vec<String> = (0..25).map(|h| format!("\"{h:02}:{h:02}\"")).collect();
    let ruins = [
        "[]".to_owned(),
        r#"["24:00"]"#.to_owned(),
        r#"["12:60"]"#.to_owned(),
        r#"["4:00"]"#.to_owned(),
        r#"["04:0"]"#.to_owned(),
        r#"["0400"]"#.to_owned(),
        r#"["ab:cd"]"#.to_owned(),
        r#"["04:00 "]"#.to_owned(),
        r#"["04:00","04:00"]"#.to_owned(),
        format!("[{}]", vinte_e_cinco.join(",")),
    ];
    for ruim in ruins {
        invalido(&format!(r#","agenda":[{{"horarios":{ruim}}}]"#));
    }
    // sem `horarios`: o serde recusa (obrigatório)
    invalido(r#","agenda":[{"dias_do_mes":[1]}]"#);
}

#[test]
fn dias_do_mes_vao_de_1_a_31_sem_repetir_nem_vazio() {
    for ok in ["[1]", "[31]", "[1,15,31]"] {
        manifesto(
            "x",
            &format!(r#","agenda":[{{"horarios":["04:00"],"dias_do_mes":{ok}}}]"#),
        );
    }
    for ruim in ["[]", "[0]", "[32]", "[1,1]", "[-1]", "[256]", r#"["1"]"#] {
        invalido(&format!(
            r#","agenda":[{{"horarios":["04:00"],"dias_do_mes":{ruim}}}]"#
        ));
    }
}

#[test]
fn parametros_da_entrada_so_os_declarados_e_do_tipo_certo() {
    let decl = PARAMETROS;
    let com = |p: &str| format!(r#","agenda":[{{"horarios":["04:00"],"parametros":{p}}}],{decl}"#);
    manifesto("x", &com(r#"{"de":"-1","ate":"0"}"#));
    manifesto(
        "x",
        &com(r#"{"de":"2026-01","ate":"2026-03","forcar":true}"#),
    );
    for ruim in [
        r#"{"mes":"0"}"#,           // não declarado
        r#"{"de":true}"#,           // mês não é booleano
        r#"{"forcar":"true"}"#,     // booleano não é texto
        r#"{"de":"ontem"}"#,        // mês ilegível
        r#"{"de":"2026-13"}"#,      // mês fora do calendário
        r#"{"de":1}"#,              // número
        r#"{"de":"0","ate":"-1"}"#, // de depois de ate
        r#"{"de":"-61"}"#,          // mais de 60 meses (até = padrão "0")
    ] {
        invalido(&com(ruim));
    }
    // sem declaração nenhuma, qualquer parâmetro de entrada é erro
    invalido(r#","agenda":[{"horarios":["04:00"],"parametros":{"de":"0"}}]"#);
}

#[test]
fn declaracao_de_parametros_nomes_tipos_e_limite() {
    manifesto(
        "x",
        r#","parametros":[{"nome":"a_1","tipo":"mes"},{"nome":"b","tipo":"booleano"}]"#,
    );
    for ruim in [
        r#"[{"nome":"A","tipo":"mes"}]"#,
        r#"[{"nome":"1a","tipo":"mes"}]"#,
        r#"[{"nome":"a-b","tipo":"mes"}]"#,
        r#"[{"nome":"a","tipo":"texto"}]"#,
        r#"[{"nome":"a","tipo":"mes","padrao":true}]"#,
        r#"[{"nome":"a","tipo":"booleano","padrao":"sim"}]"#,
        r#"[{"nome":"a","tipo":"mes"},{"nome":"a","tipo":"mes"}]"#,
        r#"[{"nome":"a","tipo":"mes","extra":1}]"#,
    ] {
        invalido(&format!(r#","parametros":{ruim}"#));
    }
    let nove: Vec<String> = (0..9)
        .map(|i| format!(r#"{{"nome":"p{i}","tipo":"mes"}}"#))
        .collect();
    invalido(&format!(r#","parametros":[{}]"#, nove.join(",")));
    manifesto("x", &format!(r#","parametros":[{}]"#, nove[..8].join(",")));
}

// ------------------------------------------------------------------------------------------
// Migração do banco
// ------------------------------------------------------------------------------------------

/// Leva o banco recém-criado ao esquema de ANTES da spec 18: sem as colunas novas e com o
/// `CHECK` de seis estados.
fn rebaixar_para_o_esquema_antigo(caminho: &std::path::Path) {
    let c = Connection::open(caminho).unwrap();
    c.execute_batch(
        "PRAGMA foreign_keys = OFF;
         DROP TABLE execucao_escrita;
         CREATE TABLE execucao_escrita (
           id            INTEGER PRIMARY KEY,
           roteiro_id    INTEGER NOT NULL REFERENCES roteiro_escrito(id),
           versao_id     INTEGER NOT NULL REFERENCES roteiro_escrito_versao(id),
           gatilho       TEXT NOT NULL,
           iniciada_em   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
           iniciada_unix INTEGER NOT NULL DEFAULT (strftime('%s','now')),
           concluida_em  TEXT,
           estado        TEXT NOT NULL DEFAULT 'rodando' CHECK (estado IN
                           ('rodando','aguardando_usuario','concluida','falhou','bloqueada','cancelada')),
           chamada       INTEGER NOT NULL DEFAULT 0,
           motivo        TEXT,
           baixados      INTEGER NOT NULL DEFAULT 0,
           novos         INTEGER NOT NULL DEFAULT 0,
           continuar     INTEGER NOT NULL DEFAULT 0,
           pid           INTEGER
         );
         CREATE INDEX execucao_escrita_roteiro ON execucao_escrita(roteiro_id, id);
         ALTER TABLE roteiro_escrito DROP COLUMN suspenso_unix;
         ALTER TABLE roteiro_escrito DROP COLUMN suspenso_motivo;",
    )
    .unwrap();
}

fn colunas(c: &Connection, tabela: &str) -> Vec<String> {
    let mut st = c.prepare(&format!("PRAGMA table_info({tabela})")).unwrap();
    st.query_map([], |l| l.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn banco_antigo_migra_sem_perder_nada_e_mantem_os_gatilhos() {
    let dir = TempDir::new().unwrap();
    let caminho = dir.path().join("roteiros.sqlite");
    drop(BancoDeRoteiros::abrir(&caminho).unwrap());
    rebaixar_para_o_esquema_antigo(&caminho);
    {
        let c = Connection::open(&caminho).unwrap();
        assert!(!colunas(&c, "execucao_escrita").contains(&"entrada_agenda".to_owned()));
        c.execute_batch(
            "INSERT INTO roteiro_escrito (id, nome) VALUES (1, 'antigo');
             INSERT INTO roteiro_escrito_versao
               (id, roteiro_id, numero, manifesto, fonte, checksum, estado, autor)
               VALUES (1, 1, 1, '{}', 'x', 'abc', 'ativo', 'usuario');
             INSERT INTO execucao_escrita (id, roteiro_id, versao_id, gatilho, estado, baixados, novos, motivo, pid)
               VALUES (7, 1, 1, 'janela', 'concluida', 3, 2, 'ok', 4242),
                      (8, 1, 1, 'agenda', 'falhou', 0, 0, 'caiu', NULL);
             INSERT INTO evento_escrito_execucao (execucao_id, tipo, mensagem)
               VALUES (7, 'registro', 'um'), (7, 'registro', 'dois'), (8, 'aviso', 'três');",
        )
        .unwrap();
    }

    // Abre com o código novo: migra.
    let banco = BancoDeRoteiros::abrir(&caminho).unwrap();
    // E de novo (idempotente).
    drop(BancoDeRoteiros::abrir(&caminho).unwrap());

    let c = Connection::open(&caminho).unwrap();
    for coluna in ["entrada_agenda", "previsto_unix", "parametros_json"] {
        assert!(
            colunas(&c, "execucao_escrita").contains(&coluna.to_owned()),
            "{coluna}"
        );
    }
    for coluna in ["suspenso_unix", "suspenso_motivo"] {
        assert!(
            colunas(&c, "roteiro_escrito").contains(&coluna.to_owned()),
            "{coluna}"
        );
    }
    // As linhas antigas continuam, com o que tinham.
    let h = banco.historico_escrito(Some("antigo"), 10).unwrap();
    assert_eq!(h.len(), 2);
    let sete = h.iter().find(|e| e.id == 7).unwrap();
    assert_eq!(sete.estado, EstadoDaExecucao::Concluida);
    assert_eq!((sete.baixados, sete.novos), (3, 2));
    assert_eq!(sete.motivo.as_deref(), Some("ok"));
    // Os eventos NÃO foram apagados em cascata pela reconstrução.
    assert_eq!(banco.eventos_escritos(7).unwrap().len(), 2);
    assert_eq!(banco.eventos_escritos(8).unwrap().len(), 1);
    let inicio = banco.inicio_da_execucao_escrita(7).unwrap();
    assert_eq!(inicio, InicioDaExecucao::default());
    // A integridade referencial está de pé e as chaves voltaram a valer.
    let ruins: i64 = c
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |l| {
            l.get(0)
        })
        .unwrap();
    assert_eq!(ruins, 0);
    // O estado novo cabe.
    let id = c
        .execute(
            "INSERT INTO execucao_escrita (roteiro_id, versao_id, gatilho, estado)
             VALUES (1, 1, 'agenda', 'entrada_recusada')",
            [],
        )
        .unwrap();
    assert_eq!(id, 1);
    // Os gatilhos de imutabilidade das versões seguem valendo.
    let e = c
        .execute(
            "UPDATE roteiro_escrito_versao SET fonte = 'outra' WHERE id = 1",
            [],
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("nunca é editada"), "{e}");
    let e = c
        .execute("DELETE FROM roteiro_escrito_versao WHERE id = 1", [])
        .unwrap_err()
        .to_string();
    assert!(e.contains("nunca é apagada"), "{e}");
    // E uma execução nova continua funcionando no banco migrado.
    banco
        .suspender_agenda_escrita("antigo", "teste", 1)
        .unwrap();
    assert!(banco.agenda_suspensa_escrita("antigo").unwrap().is_some());
}

#[test]
fn estado_entrada_recusada_nasce_no_banco_novo() {
    let dir = TempDir::new().unwrap();
    let (amb, _c) = ambiente(&dir);
    let banco = amb.banco().unwrap();
    let m = manifesto("rot-a", r#","agenda":[{"horarios":["04:00"]}]"#);
    let v = ativar(&amb, &m, "navegador.registrar('a')");
    let id = banco
        .iniciar_execucao_escrita(&v.roteiro, v.numero, "janela")
        .unwrap();
    banco
        .finalizar_escrita(id, EstadoDaExecucao::EntradaRecusada, Some("motivo"), 0, 0)
        .unwrap();
    let h = banco.historico_escrito(Some("rot-a"), 1).unwrap();
    assert_eq!(h[0].estado, EstadoDaExecucao::EntradaRecusada);
    assert_eq!(h[0].estado.texto(), "entrada_recusada");
}

// ------------------------------------------------------------------------------------------
// Quem está devido
// ------------------------------------------------------------------------------------------

const AGENDA_DUAS: &str = r##","agenda":[
    {"horarios":["04:00","12:00","20:00"]},
    {"horarios":["09:00"],"dias_do_mes":[20,30,1],"parametros":{"de":"-1"}}],"##;

fn manifesto_da_agenda() -> Manifesto {
    manifesto("rot-a", &format!("{AGENDA_DUAS}{PARAMETROS}"))
}

#[test]
fn devidos_respeitam_a_ativacao_a_ultima_execucao_e_a_ordem_da_lista() {
    let dir = TempDir::new().unwrap();
    let (amb, _c) = ambiente(&dir);
    let v = ativar(&amb, &manifesto_da_agenda(), "navegador.registrar('a')");
    fixar_ativacao(&amb, l(2026, 11, 25, 10, 0));
    let banco = amb.banco().unwrap();
    let devidos = |agora: i64| banco.escritos_devidos_agendados(agora).unwrap();

    // Ativada às 10:00 do dia 25: o das 04:00 (e o "dia 20") ficaram para trás.
    assert!(devidos(l(2026, 11, 25, 10, 30)).is_empty());
    // 12:00: a entrada 0 vence.
    let d = devidos(l(2026, 11, 25, 12, 0));
    assert_eq!(d.len(), 1);
    assert_eq!(
        (d[0].entrada, d[0].previsto_unix),
        (Some(0), Some(l(2026, 11, 25, 12, 0)))
    );
    assert_eq!(d[0].nome, "rot-a");
    assert!(d[0].parametros.is_empty());

    // Rodou às 12:00:30: o das 12:00 está cumprido, o das 20:00 ainda não venceu.
    rodou(&amb, &v, 0, l(2026, 11, 25, 12, 0) + 30);
    assert!(devidos(l(2026, 11, 25, 19, 59)).is_empty());
    assert_eq!(devidos(l(2026, 11, 25, 20, 0)).len(), 1);
    // Execução MANUAL (sem entrada) não conta como a execução da entrada.
    let manual = banco
        .iniciar_execucao_escrita(&v.roteiro, v.numero, "janela")
        .unwrap();
    banco
        .finalizar_escrita(manual, EstadoDaExecucao::Concluida, None, 0, 0)
        .unwrap();
    assert_eq!(devidos(l(2026, 11, 25, 20, 0)).len(), 1);

    // Catch-up: voltou no dia 30 às 10h depois de tudo isso. UMA vez cada, na ordem da lista,
    // com o instante mais recente e os parâmetros da entrada.
    let d = devidos(l(2026, 11, 30, 10, 0));
    assert_eq!(
        d.iter()
            .map(|x| (x.entrada, x.previsto_unix))
            .collect::<Vec<_>>(),
        vec![
            (Some(0), Some(l(2026, 11, 30, 4, 0))),
            (Some(1), Some(l(2026, 11, 30, 9, 0))),
        ]
    );
    assert_eq!(
        d[1].parametros.get("de"),
        Some(&ValorBruto::Texto("-1".into()))
    );
    // Só os nomes, sem repetir.
    assert_eq!(
        banco.escritos_devidos(l(2026, 11, 30, 10, 0)).unwrap(),
        vec!["rot-a"]
    );
}

#[test]
fn rodando_ou_desativado_ou_rascunho_nao_e_devido() {
    let dir = TempDir::new().unwrap();
    let (amb, _c) = ambiente(&dir);
    let banco = amb.banco().unwrap();
    let m = manifesto("rot-a", r#","agenda":[{"horarios":["04:00"]}]"#);
    let v = ativar(&amb, &m, "navegador.registrar('a')");
    fixar_ativacao(&amb, l(2026, 11, 1, 0, 0));
    // um rascunho com agenda também
    banco
        .importar_escrito(
            &manifesto("rot-d", r#","agenda":[{"horarios":["04:00"]}]"#),
            "navegador.registrar('a')",
            &AutorEscrito::Usuario,
            "",
        )
        .unwrap();
    let agora = l(2026, 11, 25, 10, 0);
    assert_eq!(banco.escritos_devidos(agora).unwrap(), vec!["rot-a"]);
    banco
        .iniciar_execucao_escrita(&v.roteiro, v.numero, "janela")
        .unwrap();
    assert!(banco.escritos_devidos(agora).unwrap().is_empty(), "rodando");
}

#[test]
fn agenda_horas_antiga_segue_valendo() {
    let dir = TempDir::new().unwrap();
    let (amb, _c) = ambiente(&dir);
    let banco = amb.banco().unwrap();
    let m = manifesto("rot-a", r#","agenda_horas":24"#);
    ativar(&amb, &m, "navegador.registrar('a')");
    let agora = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap();
    let d = banco.escritos_devidos_agendados(agora).unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].entrada, d[0].previsto_unix), (None, None));
}

#[test]
fn suspensa_nao_e_devida_e_so_retoma_pelo_usuario() {
    let dir = TempDir::new().unwrap();
    let (amb, _c) = ambiente(&dir);
    ativar(&amb, &manifesto_da_agenda(), "navegador.registrar('a')");
    fixar_ativacao(&amb, l(2026, 11, 1, 0, 0));
    let banco = amb.banco().unwrap();
    let agora = l(2026, 11, 25, 13, 0);
    assert_eq!(banco.escritos_devidos(agora).unwrap(), vec!["rot-a"]);

    banco
        .suspender_agenda_escrita("rot-a", "o portal pediu\npara trocar a senha", agora)
        .unwrap();
    assert!(banco.escritos_devidos(agora).unwrap().is_empty());
    let s = banco.agenda_suspensa_escrita("rot-a").unwrap().unwrap();
    assert_eq!(s.desde_unix, agora);
    assert_eq!(
        s.motivo, "o portal pediu para trocar a senha",
        "sem controle"
    );
    // o histórico de estados da versão registra
    let d = banco.mostrar("rot-a").unwrap();
    assert!(d.eventos.iter().any(|e| e.5.contains("agenda suspensa")));

    assert!(banco.retomar_agenda_escrita("rot-a").unwrap());
    assert!(banco.agenda_suspensa_escrita("rot-a").unwrap().is_none());
    assert_eq!(banco.escritos_devidos(agora).unwrap(), vec!["rot-a"]);
    // retomar o que não está suspenso não faz nada
    assert!(!banco.retomar_agenda_escrita("rot-a").unwrap());
    assert!(banco.retomar_agenda_escrita("nao-existe").is_err());
    assert!(
        banco
            .suspender_agenda_escrita("nao-existe", "x", 1)
            .is_err()
    );
    let d = banco.mostrar("rot-a").unwrap();
    assert!(d.eventos.iter().any(|e| e.5 == "agenda retomada"));
    // motivo longo é cortado em 200
    banco
        .suspender_agenda_escrita("rot-a", &"a".repeat(500), agora)
        .unwrap();
    assert_eq!(
        banco
            .agenda_suspensa_escrita("rot-a")
            .unwrap()
            .unwrap()
            .motivo
            .chars()
            .count(),
        200
    );
}

// ------------------------------------------------------------------------------------------
// Serviço: parâmetros no Luau, entrada recusada, baixa por conteúdo
// ------------------------------------------------------------------------------------------

#[test]
fn o_roteiro_recebe_os_parametros_resolvidos_e_o_historico_os_guarda() {
    let dir = TempDir::new().unwrap();
    let (amb, _c) = ambiente(&dir);
    let fonte = r##"
        navegador.registrar(parametros.de .. " " .. parametros.ate .. " " .. tostring(parametros.forcar))
        navegador.registrar(table.concat(util.meses(parametros.de, parametros.ate), ","))
    "##;
    let v = ativar(&amb, &manifesto("rot-a", &format!(",{PARAMETROS}")), fonte);
    let mut n = painel(b"%PDF-1.4 um");
    let pedido = PedidoDeExecucao {
        entregues: [("de".to_owned(), ValorBruto::Texto("-1".into()))].into(),
        ..PedidoDeExecucao::default()
    };
    let (rel, id) = executar(&amb, &v, &mut n, &pedido, AGORA);
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(
        rel.registros,
        vec!["2026-10 2026-11 false", "2026-10,2026-11"],
        "24/11 às 22h em UTC−3 é novembro (não dezembro)"
    );
    let inicio = amb.banco().unwrap().inicio_da_execucao_escrita(id).unwrap();
    assert_eq!(
        inicio.parametros_json.as_deref(),
        Some(r#"{"ate":"2026-11","de":"2026-10","forcar":false}"#)
    );
}

#[test]
fn a_agenda_resolve_o_mes_relativo_a_partir_do_instante_previsto() {
    let v = manifesto("rot-a", &format!(",{PARAMETROS}"));
    // Previsto em 30/11 às 21h (já é 1/12 em UTC): ainda é novembro, e o relativo "0" também.
    let pedido = PedidoDeExecucao {
        previsto_unix: Some(l(2026, 11, 30, 21, 0)),
        ..PedidoDeExecucao::default()
    };
    // O relógio já está em dezembro (o PC voltou depois): o previsto manda.
    let r = resolver_pedido(&v, &pedido, l(2026, 12, 2, 10, 0)).unwrap();
    assert_eq!(
        r.inicio.parametros_json.as_deref(),
        Some(r#"{"ate":"2026-11","de":"2026-11","forcar":false}"#)
    );
    // Manual (sem previsto): conta do agora.
    let r = resolver_pedido(&v, &PedidoDeExecucao::default(), l(2026, 12, 2, 10, 0)).unwrap();
    assert_eq!(
        r.inicio.parametros_json.as_deref(),
        Some(r#"{"ate":"2026-12","de":"2026-12","forcar":false}"#)
    );
}

#[test]
fn pedido_invalido_e_erro_antes_de_rodar() {
    let m = manifesto("rot-a", &format!(",{PARAMETROS}"));
    let e = |ent: &[(&str, ValorBruto)]| {
        resolver_pedido(
            &m,
            &PedidoDeExecucao {
                entregues: ent
                    .iter()
                    .map(|(n, v)| ((*n).to_owned(), v.clone()))
                    .collect(),
                ..PedidoDeExecucao::default()
            },
            AGORA,
        )
    };
    assert!(
        e(&[
            ("de", ValorBruto::Texto("0".into())),
            ("ate", ValorBruto::Texto("-1".into()))
        ])
        .is_err()
    );
    assert!(e(&[("mes", ValorBruto::Texto("0".into()))]).is_err());
    assert!(
        e(&[("de", ValorBruto::Texto("2020-01".into()))]).is_err(),
        "mais de 60 meses"
    );
    assert!(e(&[("de", ValorBruto::Texto("-3".into()))]).is_ok());
    // sem parâmetros declarados a tabela é vazia e o `forcar` do pedido não vira parâmetro
    let sem = manifesto("rot-b", "");
    let r = resolver_pedido(
        &sem,
        &PedidoDeExecucao {
            forcar: true,
            ..PedidoDeExecucao::default()
        },
        AGORA,
    )
    .unwrap();
    assert!(r.parametros.vazio());
    assert!(r.forcar);
    assert_eq!(
        r.inicio.parametros_json.as_deref(),
        Some(r#"{"forcar":true}"#)
    );
    // declarado: o roteiro o vê
    let r = resolver_pedido(
        &m,
        &PedidoDeExecucao {
            forcar: true,
            ..PedidoDeExecucao::default()
        },
        AGORA,
    )
    .unwrap();
    assert!(r.forcar);
    assert_eq!(
        r.parametros.obter("forcar"),
        Some(&bancada_navegador::luau::ValorDeParametro::Booleano(true))
    );
}

#[test]
fn entrada_recusada_suspende_a_agenda_e_nao_retoma_sozinha() {
    let dir = TempDir::new().unwrap();
    let (amb, _c) = ambiente(&dir);
    let fonte = r##"
        navegador.registrar("antes")
        navegador.entrada_recusada("o portal pediu para trocar a senha")
    "##;
    let m = manifesto("rot-a", r#","agenda":[{"horarios":["04:00"]}]"#);
    let v = ativar(&amb, &m, fonte);
    fixar_ativacao(&amb, l(2026, 11, 1, 0, 0));
    let banco = amb.banco().unwrap();
    assert_eq!(banco.escritos_devidos(AGORA).unwrap(), vec!["rot-a"]);

    let mut n = painel(b"%PDF-1.4 um");
    let (rel, id) = executar(&amb, &v, &mut n, &PedidoDeExecucao::default(), AGORA);
    assert_eq!(
        rel.entrada_recusada.as_deref(),
        Some("o portal pediu para trocar a senha")
    );
    assert!(matches!(rel.fim, Fim::Falhou { .. }), "{:?}", rel.fim);
    let h = banco.historico_escrito(Some("rot-a"), 1).unwrap();
    assert_eq!(h[0].id, id);
    assert_eq!(h[0].estado, EstadoDaExecucao::EntradaRecusada);
    assert!(h[0].motivo.as_deref().unwrap().contains("trocar a senha"));
    assert!(
        banco
            .eventos_escritos(id)
            .unwrap()
            .iter()
            .any(|e| e.1 == "entrada_recusada")
    );
    let s = banco.agenda_suspensa_escrita("rot-a").unwrap().unwrap();
    assert_eq!(s.desde_unix, AGORA);
    assert!(s.motivo.contains("trocar a senha"));
    assert!(banco.escritos_devidos(AGORA + 86_400).unwrap().is_empty());

    // Execução manual de roteiro suspenso é permitida e, se der certo, NÃO retoma.
    let fonte_ok = "navegador.registrar('ok')";
    let m2 = manifesto("rot-b", r#","agenda":[{"horarios":["04:00"]}]"#);
    let v2 = ativar(&amb, &m2, fonte_ok);
    banco
        .suspender_agenda_escrita("rot-b", "antiga", 1)
        .unwrap();
    let (rel, _) = executar(&amb, &v2, &mut n, &PedidoDeExecucao::default(), AGORA);
    assert_eq!(rel.fim, Fim::Concluida);
    assert!(banco.agenda_suspensa_escrita("rot-b").unwrap().is_some());
}

#[test]
fn entrada_recusada_no_modo_teste_nao_suspende_nada() {
    // O modo teste (o `testar`) nunca mexe na agenda. Aqui o roteiro roda com as opções de
    // teste (host lógico → base real) e recusa a entrada.
    let dir = TempDir::new().unwrap();
    let (amb, _c) = ambiente(&dir);
    let banco = amb.banco().unwrap();
    let m = manifesto("rot-t", r#","agenda":[{"horarios":["04:00"]}]"#);
    let n = banco
        .importar_escrito(
            &m,
            "navegador.entrada_recusada('falso')",
            &AutorEscrito::Usuario,
            "",
        )
        .unwrap()
        .numero;
    let v = banco.versao_escrita("rot-t", n).unwrap();
    let id = banco.iniciar_execucao_escrita("rot-t", n, "teste").unwrap();
    let opcoes = OpcoesEscrito {
        mapa_de_hosts: vec![("portal.exemplo.test".into(), "http://127.0.0.1:1".into())],
        aceitar_http_loopback: true,
    };
    let relogio = RelogioFixo(AGORA);
    let pausa = PausaFalsa::com(&[]);
    let resolvida = resolver_pedido(&v.manifesto, &PedidoDeExecucao::default(), AGORA).unwrap();
    let rel = executar_escrito_em_com(
        &amb,
        &mut painel(b"x"),
        &v,
        id,
        &Servicos {
            senhas: &Senhas,
            caixa: None,
            relogio: &relogio,
            pausa: Some(&pausa),
            opcoes: &opcoes,
        },
        &AtomicBool::new(false),
        &resolvida,
    )
    .unwrap();
    assert_eq!(rel.entrada_recusada.as_deref(), Some("falso"));
    assert!(banco.agenda_suspensa_escrita("rot-t").unwrap().is_none());
}

#[test]
fn a_passada_da_agenda_roda_cada_entrada_devida_uma_vez_com_os_parametros_dela() {
    let dir = TempDir::new().unwrap();
    let (amb, _c) = ambiente(&dir);
    let v = ativar(&amb, &manifesto_da_agenda(), "navegador.registrar('a')");
    fixar_ativacao(&amb, l(2026, 11, 1, 0, 0));
    let banco = amb.banco().unwrap();
    let agora = l(2026, 11, 30, 10, 0);
    let mut vistos = Vec::new();
    let r = executar_escritos_agendados_com(&amb, agora, &AtomicBool::new(false), &mut |d| {
        vistos.push((d.entrada, d.previsto_unix, d.parametros.clone()));
        // registra como o serviço real: a execução leva a entrada e o previsto
        let pedido = sbf_roteiros::servico::pedido_da_agenda(d);
        let resolvida = resolver_pedido(&v.manifesto, &pedido, agora).unwrap();
        let id = banco
            .iniciar_execucao_escrita_com(&d.nome, d.numero, "agenda", &resolvida.inicio)
            .unwrap();
        banco
            .finalizar_escrita(id, EstadoDaExecucao::Concluida, None, 0, 0)
            .unwrap();
        Connection::open(amb.arquivo_do_banco())
            .unwrap()
            .execute(
                "UPDATE execucao_escrita SET iniciada_unix = ?2 WHERE id = ?1",
                rusqlite::params![id, agora],
            )
            .unwrap();
        Ok(sbf_roteiros::servico::RelatorioEscrito {
            fim: Fim::Concluida,
            baixados: 0,
            novos: 0,
            descartados: 0,
            registros: vec![],
            entrada_recusada: None,
        })
    })
    .unwrap();
    assert_eq!(r.executados, 2);
    assert_eq!(r.concluidos, 2);
    assert_eq!(vistos.len(), 2);
    assert_eq!(vistos[0].0, Some(0));
    assert_eq!(vistos[1].0, Some(1));
    // A entrada 1 pediu "de = -1" e o previsto é 30/11: o de é 2026-10.
    let ultima = banco.historico_escrito(Some("rot-a"), 1).unwrap();
    let ini = banco.inicio_da_execucao_escrita(ultima[0].id).unwrap();
    assert_eq!(ini.entrada, Some(1));
    assert_eq!(ini.previsto_unix, Some(l(2026, 11, 30, 9, 0)));
    assert_eq!(
        ini.parametros_json.as_deref(),
        Some(r#"{"ate":"2026-11","de":"2026-10","forcar":false}"#)
    );
    // Passou: ninguém mais é devido até o próximo horário.
    assert!(banco.escritos_devidos(agora).unwrap().is_empty());
}

#[test]
fn baixa_por_conteudo_descarta_o_repetido_e_forcar_baixa_de_novo() {
    let dir = TempDir::new().unwrap();
    let (amb, conn) = ambiente(&dir);
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/painel")
        navegador.baixar("#baixar", "contracheque")
    "##;
    let v = ativar(&amb, &manifesto("rot-a", &format!(",{PARAMETROS}")), fonte);
    let contar = || -> i64 {
        conn.query_row("SELECT COUNT(*) FROM achado", [], |l| l.get(0))
            .unwrap()
    };

    // 1ª: o arquivo é novo.
    let (rel, _) = executar(
        &amb,
        &v,
        &mut painel(b"%PDF-1.4 do dia 20"),
        &PedidoDeExecucao::default(),
        AGORA,
    );
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!((rel.baixados, rel.novos, rel.descartados), (1, 1, 0));
    assert_eq!(contar(), 1);

    // 2ª: o mesmo conteúdo no mesmo lugar do portal: descartado, sem achado novo.
    let (rel, id2) = executar(
        &amb,
        &v,
        &mut painel(b"%PDF-1.4 do dia 20"),
        &PedidoDeExecucao::default(),
        AGORA,
    );
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!((rel.baixados, rel.novos, rel.descartados), (1, 0, 1));
    assert_eq!(contar(), 1);
    // O histórico guarda a nota.
    let eventos = amb.banco().unwrap().eventos_escritos(id2).unwrap();
    assert!(
        eventos
            .iter()
            .any(|e| e.1 == "descartado" && e.2.contains("já estava no inventário")),
        "{eventos:?}"
    );

    // 3ª: conteúdo diferente (o contracheque do dia 30 substituiu o do dia 20): é novo.
    let (rel, _) = executar(
        &amb,
        &v,
        &mut painel(b"%PDF-1.4 do dia 30"),
        &PedidoDeExecucao::default(),
        AGORA,
    );
    assert_eq!((rel.baixados, rel.novos, rel.descartados), (1, 1, 0));
    assert_eq!(contar(), 2);

    // 4ª: a mesma coisa, mas FORÇADA: não descarta.
    let forcado = PedidoDeExecucao {
        forcar: true,
        ..PedidoDeExecucao::default()
    };
    let (rel, _) = executar(
        &amb,
        &v,
        &mut painel(b"%PDF-1.4 do dia 30"),
        &forcado,
        AGORA,
    );
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(rel.baixados, 1);
    assert_eq!(rel.descartados, 0, "forçar não descarta");
}
