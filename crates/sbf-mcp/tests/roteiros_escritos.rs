//! As ferramentas de roteiros escritos (spec 16, RE-19): o MCP só PROPÕE. Roteiros e
//! manifestos fictícios (`exemplo.test`), num `roteiros.sqlite` temporário.
#![allow(clippy::unwrap_used)]

#[path = "../../sbf-triagem/tests/ajuda/mod.rs"]
#[allow(unused_imports)]
mod ajuda;

use std::path::PathBuf;

use ajuda::{Banco, banco_limpo};
use rmcp::ServerHandler;
use sbf_mcp::Nucleo;
use sbf_mcp::roteiros_escritos::{
    FERRAMENTAS, MENSAGEM_DE_PROPOSTA, PedidoDeRoteiroEscrito, PedidoDeVerRoteiroEscrito,
    RoteirosEscritos,
};
use sbf_mcp::servidor::Servidor;
use sbf_roteiros::banco::{
    AutorEscrito, BancoDeRoteiros, Confirmacao, EstadoEscrito, ResultadoDoTeste,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const FONTE: &str =
    "navegador.abrir(\"https://portal.exemplo.test/entrar\")\nnavegador.registrar(\"ok\")\n";

struct Cenario {
    banco: Banco,
    dir: TempDir,
    caminho: PathBuf,
    nucleo: Nucleo,
}

/// `existente`: se o `roteiros.sqlite` já existe quando o MCP liga.
fn cenario(existente: bool) -> Cenario {
    let banco = banco_limpo();
    let dir = tempfile::tempdir().unwrap();
    let caminho = dir.path().join("dados").join("roteiros.sqlite");
    if existente {
        drop(BancoDeRoteiros::abrir(&caminho).unwrap());
    }
    let nucleo = Nucleo::novo(&banco.caminho, None)
        .unwrap()
        .com_roteiros_escritos(caminho.clone());
    Cenario {
        banco,
        dir,
        caminho,
        nucleo,
    }
}

impl Cenario {
    fn roteiros(&self) -> &RoteirosEscritos {
        self.nucleo.roteiros_escritos().unwrap()
    }

    fn banco_de_roteiros(&self) -> BancoDeRoteiros {
        BancoDeRoteiros::abrir(&self.caminho).unwrap()
    }

    fn pasta_de_backups(&self) -> PathBuf {
        self.banco.caminho.parent().unwrap().join("backups/mcp")
    }

    fn saidas(&self, destinatario: &str) -> i64 {
        self.banco
            .conexao()
            .query_row(
                "SELECT COUNT(*) FROM saida_ia WHERE destinatario = ?1",
                [destinatario],
                |l| l.get(0),
            )
            .unwrap()
    }
}

fn manifesto() -> Value {
    json!({
        "nome": "contracheque-exemplo",
        "descricao": "Baixa o contracheque do portal fictício",
        "url_inicial": "https://portal.exemplo.test/entrar",
        "hosts": ["portal.exemplo.test"],
        "rotulo_senha": "senha:portal-exemplo",
        "rotulo_cpf": null,
        "mfa": { "tipo": "nenhum" },
        "movimenta_dinheiro": false,
        "classe": "consulta",
        "agenda_horas": 168
    })
}

fn pedido(manifesto: Value, fonte: &str) -> PedidoDeRoteiroEscrito {
    PedidoDeRoteiroEscrito {
        manifesto,
        fonte: fonte.to_owned(),
        motivo: "primeira versão fictícia".to_owned(),
    }
}

fn ver(nome: &str, versao: Option<u32>) -> PedidoDeVerRoteiroEscrito {
    PedidoDeVerRoteiroEscrito {
        nome: nome.to_owned(),
        versao,
    }
}

// ------------------------------------------------------------------------ propor

#[test]
fn propor_cria_um_rascunho_com_autor_mcp_e_nao_ativa() {
    let c = cenario(false);
    let r = c.roteiros().propor(&pedido(manifesto(), FONTE)).unwrap();
    assert_eq!(r["criado"], true);
    assert_eq!(r["nome"], "contracheque-exemplo");
    assert_eq!(r["versao"], 1);
    assert_eq!(r["estado"], "rascunho");
    assert_eq!(r["autor"], "mcp");
    assert_eq!(r["mensagem"], MENSAGEM_DE_PROPOSTA);
    assert!(
        r["mensagem"]
            .as_str()
            .unwrap()
            .contains("sbf roteiros escrito ativar")
    );
    assert_eq!(r["checksum"].as_str().unwrap().len(), 64);

    let b = c.banco_de_roteiros();
    let v = b.versao_escrita("contracheque-exemplo", 1).unwrap();
    assert_eq!(v.autor, AutorEscrito::Mcp);
    assert_eq!(v.estado, EstadoEscrito::Rascunho);
    assert_eq!(v.fonte, FONTE);
    assert_eq!(v.checksum, r["checksum"]);
    assert!(
        b.versao_ativa_escrita("contracheque-exemplo")
            .unwrap()
            .is_none()
    );
    // O histórico registra quem propôs.
    let h = b.mostrar("contracheque-exemplo").unwrap().eventos;
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].4, "mcp");
}

#[test]
fn reimportar_o_mesmo_conteudo_nao_duplica() {
    let c = cenario(false);
    let a = c.roteiros().propor(&pedido(manifesto(), FONTE)).unwrap();
    let b = c.roteiros().propor(&pedido(manifesto(), FONTE)).unwrap();
    assert_eq!(b["criado"], false);
    assert_eq!(b["versao"], 1);
    assert_eq!(b["checksum"], a["checksum"]);
    assert!(b["mensagem"].as_str().unwrap().contains("já existe"));
    assert_eq!(
        c.banco_de_roteiros()
            .mostrar("contracheque-exemplo")
            .unwrap()
            .versoes
            .len(),
        1
    );
    // Conteúdo diferente vira a versão 2, também rascunho.
    let d = c
        .roteiros()
        .propor(&pedido(
            manifesto(),
            &format!("{FONTE}navegador.registrar(\"fim\")\n"),
        ))
        .unwrap();
    assert_eq!(d["criado"], true);
    assert_eq!(d["versao"], 2);
}

#[test]
fn proposta_nova_nao_mexe_na_versao_ativa() {
    let c = cenario(false);
    c.roteiros().propor(&pedido(manifesto(), FONTE)).unwrap();
    let b = c.banco_de_roteiros();
    b.marcar_testado(
        "contracheque-exemplo",
        1,
        &ResultadoDoTeste {
            passou: true,
            resumo: "passou no site falso".to_owned(),
        },
    )
    .unwrap();
    // a versão é do MCP: o usuário digita também o host principal
    let esperado = b.esperado_para_ativar("contracheque-exemplo", 1).unwrap();
    assert!(esperado.ends_with(" portal.exemplo.test"), "{esperado}");
    let confirmacao = Confirmacao::do_terminal(true, &esperado, &esperado).unwrap();
    b.ativar("contracheque-exemplo", 1, &confirmacao).unwrap();

    let r = c
        .roteiros()
        .propor(&pedido(
            manifesto(),
            &format!("{FONTE}navegador.registrar(\"v2\")\n"),
        ))
        .unwrap();
    assert_eq!(r["versao"], 2);
    assert_eq!(r["estado"], "rascunho");
    assert_eq!(
        b.versao_ativa_escrita("contracheque-exemplo")
            .unwrap()
            .unwrap()
            .numero,
        1
    );
    let l = c
        .roteiros()
        .ver(&ver("contracheque-exemplo", None))
        .unwrap();
    assert_eq!(l["ativa"], 1);
    assert_eq!(l["versao"]["numero"], 2);
}

#[test]
fn manifesto_invalido_e_recusado_com_a_mensagem_e_nada_e_criado() {
    let c = cenario(true);
    let casos: Vec<(Value, &str)> = vec![
        (
            {
                let mut m = manifesto();
                m["mfa"] = json!({"tipo": "email", "remetente": "nao-responda@portal.exemplo.test", "formato": "\\d{6}"});
                m["movimenta_dinheiro"] = json!(true);
                m
            },
            "MFA por e-mail só para roteiro que não movimenta dinheiro",
        ),
        (
            {
                let mut m = manifesto();
                m["hosts"] = json!(["https://portal.exemplo.test"]);
                m
            },
            "sem https://",
        ),
        (
            {
                let mut m = manifesto();
                m["url_inicial"] = json!("http://portal.exemplo.test/entrar");
                m
            },
            "https://",
        ),
        (
            {
                let mut m = manifesto();
                m["nome"] = json!("Nome Invalido");
                m
            },
            "kebab-case",
        ),
        (
            {
                let mut m = manifesto();
                m.as_object_mut().unwrap().remove("movimenta_dinheiro");
                m
            },
            "movimenta_dinheiro",
        ),
        (json!("não é objeto"), "manifesto"),
    ];
    for (m, trecho) in casos {
        let e = c.roteiros().propor(&pedido(m, FONTE)).unwrap_err();
        assert!(e.contains(trecho), "esperava `{trecho}` em `{e}`");
    }
    assert!(c.banco_de_roteiros().listar_escritos().unwrap().is_empty());
    // Nem backup disparou: pedido inválido não escreve.
    assert!(!c.pasta_de_backups().exists());
}

#[test]
fn payload_gigante_e_recusado() {
    let c = cenario(false);
    // Fonte com 64 KiB + 1.
    let grande = "-".repeat(64 * 1024 + 1);
    let e = c
        .roteiros()
        .propor(&pedido(manifesto(), &grande))
        .unwrap_err();
    assert!(e.contains("64 KiB"), "{e}");
    // Manifesto com 64 KiB + 1 (descrição enorme).
    let mut m = manifesto();
    m["descricao"] = json!("a".repeat(64 * 1024 + 1));
    let e = c.roteiros().propor(&pedido(m, FONTE)).unwrap_err();
    assert!(e.contains("64 KiB"), "{e}");
    // No limite exato do fonte passa a checagem de tamanho.
    let no_limite = "-".repeat(64 * 1024);
    assert!(
        c.roteiros()
            .propor(&pedido(manifesto(), &no_limite))
            .is_ok()
    );
    // Fonte vazio e motivo longo demais.
    assert!(c.roteiros().propor(&pedido(manifesto(), "  \n")).is_err());
    let mut p = pedido(manifesto(), FONTE);
    p.motivo = "m".repeat(501);
    assert!(c.roteiros().propor(&p).unwrap_err().contains("500"));
    // Só a do limite exato foi criada.
    assert_eq!(c.banco_de_roteiros().listar_escritos().unwrap().len(), 1);
}

// ------------------------------------------------------------------------ consultar

#[test]
fn listar_e_ver_so_leem_e_mostram_manifesto_fonte_e_estado() {
    let c = cenario(false);
    // Sem arquivo: lista vazia e nada é criado.
    let vazio = c.roteiros().listar().unwrap();
    assert_eq!(vazio["total"], 0);
    assert!(
        c.roteiros()
            .ver(&ver("contracheque-exemplo", None))
            .is_err()
    );
    assert!(!c.caminho.exists());

    c.roteiros().propor(&pedido(manifesto(), FONTE)).unwrap();
    let l = c.roteiros().listar().unwrap();
    assert_eq!(l["total"], 1);
    assert_eq!(l["itens"][0]["nome"], "contracheque-exemplo");
    assert_eq!(l["itens"][0]["ativa"], Value::Null);
    assert_eq!(l["itens"][0]["estado_da_ultima"], "rascunho");

    let v = c
        .roteiros()
        .ver(&ver("contracheque-exemplo", Some(1)))
        .unwrap();
    assert_eq!(v["versao"]["fonte"], FONTE);
    assert_eq!(v["versao"]["autor"], "mcp");
    assert_eq!(
        v["versao"]["manifesto"]["rotulo_senha"],
        "senha:portal-exemplo"
    );
    assert_eq!(v["versao"]["manifesto"]["hosts"][0], "portal.exemplo.test");
    assert_eq!(v["historico"][0]["quem"], "mcp");
    assert!(
        c.roteiros()
            .ver(&ver("contracheque-exemplo", Some(9)))
            .is_err()
    );
    assert!(c.roteiros().ver(&ver("outro", None)).is_err());
    assert!(c.roteiros().ver(&ver("", None)).is_err());
}

#[test]
fn leituras_nao_fazem_backup_e_cada_chamada_grava_saida_ia() {
    let c = cenario(true);
    c.roteiros().listar().unwrap();
    assert!(c.roteiros().ver(&ver("nada", None)).is_err());
    assert!(!c.pasta_de_backups().exists());
    c.roteiros().propor(&pedido(manifesto(), FONTE)).unwrap();
    assert_eq!(c.saidas("mcp:listar_roteiros_escritos"), 1);
    assert_eq!(c.saidas("mcp:ver_roteiro_escrito"), 1);
    assert_eq!(c.saidas("mcp:propor_roteiro_escrito"), 1);
}

// ------------------------------------------------------------------------ só propõe

#[test]
fn nao_existe_ferramenta_de_ativar_executar_testar_ou_desativar() {
    let c = cenario(false);
    let nomes = Servidor::novo(c.nucleo.clone()).nomes_das_ferramentas();
    for f in FERRAMENTAS {
        assert!(nomes.iter().any(|n| n == f), "falta {f}");
    }
    // Toda ferramenta que fala de roteiro é uma das três permitidas.
    let de_roteiro: Vec<_> = nomes.iter().filter(|n| n.contains("roteiro")).collect();
    assert_eq!(de_roteiro.len(), FERRAMENTAS.len(), "{de_roteiro:?}");
    for n in &nomes {
        if n.contains("roteiro") {
            assert!(
                FERRAMENTAS.contains(&n.as_str()),
                "ferramenta de roteiro inesperada: {n}"
            );
            for proibido in [
                "ativar",
                "desativar",
                "executar",
                "rodar",
                "testar",
                "aprovar",
                "confirmar",
            ] {
                assert!(!n.contains(proibido), "{n} não pode existir");
            }
        }
    }
    // Nem uma ferramenta genérica que pudesse alcançar roteiros: as de regra falam de regra.
    for n in &nomes {
        assert!(
            !(n.starts_with("ativar_") && n != "ativar_regra"),
            "ferramenta de ativar inesperada: {n}"
        );
        assert!(
            !n.starts_with("executar_"),
            "ferramenta de executar inesperada: {n}"
        );
    }
}

#[test]
fn as_instrucoes_do_servidor_dizem_que_roteiro_escrito_so_se_propoe() {
    let c = cenario(false);
    let info = Servidor::novo(c.nucleo.clone()).get_info();
    let texto = info.instructions.unwrap();
    assert!(texto.contains("ROTEIROS ESCRITOS"), "{texto}");
    assert!(texto.contains("so PROPOE"), "{texto}");
    assert!(texto.contains("NAO existe ferramenta que ative"), "{texto}");
    assert!(texto.contains("sbf roteiros escrito ativar"), "{texto}");
}

#[test]
fn sem_roteiros_escritos_ligados_as_ferramentas_nao_aparecem() {
    let banco = banco_limpo();
    let nucleo = Nucleo::novo(&banco.caminho, None).unwrap();
    assert!(!nucleo.tem_roteiros_escritos());
    let nomes = Servidor::novo(nucleo.clone()).nomes_das_ferramentas();
    for f in FERRAMENTAS {
        assert!(!nomes.iter().any(|n| n == f), "não devia anunciar {f}");
    }
    assert!(
        nucleo
            .roteiros_escritos()
            .unwrap_err()
            .contains("não estão ligados")
    );
}

// ------------------------------------------------------------------------ backup

#[test]
fn primeira_escrita_faz_backup_do_roteiros_sqlite_existente_e_so_uma_vez() {
    let c = cenario(true);
    let b = c.banco_de_roteiros();
    drop(b);
    c.roteiros().propor(&pedido(manifesto(), FONTE)).unwrap();
    c.roteiros()
        .propor(&pedido(
            manifesto(),
            &format!("{FONTE}navegador.registrar(\"v2\")\n"),
        ))
        .unwrap();
    let copias: Vec<_> = std::fs::read_dir(c.pasta_de_backups())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(copias.len(), 1, "{copias:?}");
    assert!(copias[0].starts_with("roteiros-") && copias[0].ends_with(".sqlite"));
    // A cópia é de antes da escrita: ainda sem nenhum roteiro.
    let copia = BancoDeRoteiros::abrir(&c.pasta_de_backups().join(&copias[0])).unwrap();
    assert!(copia.listar_escritos().unwrap().is_empty());
}

#[test]
fn sem_o_arquivo_ainda_nao_ha_o_que_copiar_e_a_proposta_o_cria() {
    let c = cenario(false);
    assert!(!c.caminho.exists());
    c.roteiros().propor(&pedido(manifesto(), FONTE)).unwrap();
    assert!(c.caminho.is_file());
    assert!(!c.pasta_de_backups().exists());
}

#[test]
fn sem_backup_a_escrita_e_recusada() {
    let c = cenario(true);
    // Uma FILE no lugar da pasta `backups` impede criar a pasta de backup.
    std::fs::write(
        c.banco.caminho.parent().unwrap().join("backups"),
        "obstáculo",
    )
    .unwrap();
    let e = c
        .roteiros()
        .propor(&pedido(manifesto(), FONTE))
        .unwrap_err();
    assert!(e.contains("backup"), "{e}");
    assert!(c.banco_de_roteiros().listar_escritos().unwrap().is_empty());
}

#[test]
fn fazer_backup_sob_demanda_inclui_o_roteiros_sqlite() {
    let c = cenario(true);
    let r = c.nucleo.fazer_backup().unwrap();
    let bancos: Vec<_> = r["feitos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["banco"].as_str().unwrap().to_owned())
        .collect();
    assert!(bancos.iter().any(|b| b == "roteiros"), "{bancos:?}");
    let _ = &c.dir;
}
