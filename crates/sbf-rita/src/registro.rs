//! O registro do que sai (ADR-015, spec 15, D13): todo texto que Rita manda a um provedor
//! grava uma linha em `saida_ia` **antes** do envio. Se o registro não abre ou não grava, nada é
//! enviado (falha fechada) e o erro diz o porquê.
//!
//! **O provedor local também registra.** O ADR-015 não isenta destinatário local: diz que "o
//! app registra o que saiu, para onde e a que custo", e o nível de confiança vale para todo
//! destinatário. Um Ollama em `localhost` é só mais um provedor com custo zero, e o endereço do
//! provedor "local" pode apontar para outra máquina. Então todos registram, com destinatário
//! `rita` e nível completo (D13); no local os tokens contam e o custo fica em zero.
//!
//! A linha nunca leva conteúdo: só tamanho, provedor, modelo, tokens e custo.

use std::path::Path;

use sbf_ia::{Consumo, ErroDeIa, NivelDeConfianca, Registro, Saida};
use sbf_inventario::sugestao::RegistroDaIa;

use crate::contexto::ARQUIVO_DO_INVENTARIO;

/// O destinatário em `saida_ia` (ADR-015): todos os provedores recebem o texto completo (D13).
pub const DESTINATARIO: &str = "rita";

const NADA_FOI_ENVIADO: &str = "o registro do que sai (saida_ia, ADR-015) não abriu ou não gravou, então nada foi enviado ao provedor. Confira o inventário do cofre";

/// O erro de quem não conseguiu registrar: diz que nada foi enviado.
pub fn erro_de_registro(motivo: &str) -> ErroDeIa {
    ErroDeIa::Indisponivel(format!("{NADA_FOI_ENVIADO} ({motivo})"))
}

/// Abre o registro do inventário do cofre. Não cria o inventário: sem ele, falha.
///
/// # Errors
/// Inventário ausente ou ilegível (nada será enviado).
pub fn abrir_registro(cofre: &Path) -> Result<RegistroDaIa, ErroDeIa> {
    let caminho = cofre.join(ARQUIVO_DO_INVENTARIO);
    if !caminho.is_file() {
        return Err(erro_de_registro("inventário.sqlite não existe no cofre"));
    }
    RegistroDaIa::abrir(&caminho, None).map_err(|_| erro_de_registro("não abriu o inventário"))
}

/// Grava a linha da saída que vai acontecer. `Ok(id)` autoriza o envio.
///
/// # Errors
/// O registro recusou: quem chama não envia.
pub fn registrar_antes(
    registro: &dyn Registro,
    provedor: &str,
    modelo: &str,
    bytes: u64,
) -> Result<Option<i64>, ErroDeIa> {
    registro
        .abrir_saida(&Saida {
            destinatario: DESTINATARIO.to_owned(),
            provedor: provedor.to_owned(),
            modelo: modelo.to_owned(),
            nivel: NivelDeConfianca::Completo,
            bytes_enviados: bytes,
            consumo: Consumo::default(),
            quando: sbf_ia::agora_rfc3339(),
        })
        .map_err(|e| match e {
            ErroDeIa::Interrompido => ErroDeIa::Interrompido,
            outro => erro_de_registro(&outro.to_string()),
        })
}

/// Completa a linha com os tokens e o custo da resposta. Se falhar, a saída continua
/// registrada: só os tokens se perdem.
pub fn completar(registro: &dyn Registro, id: Option<i64>, consumo: Consumo) {
    if let Some(id) = id {
        let _ = registro.fechar_saida(id, consumo);
    }
}

/// Um inventário mínimo (só a tabela `saida_ia`) para os testes de quem usa Rita.
#[doc(hidden)]
pub fn inventario_minimo_para_testes(cofre: &Path) {
    let c = rusqlite::Connection::open(cofre.join(ARQUIVO_DO_INVENTARIO)).expect("inventário");
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS saida_ia (id INTEGER PRIMARY KEY, destinatario TEXT NOT NULL,
           provedor TEXT NOT NULL, modelo TEXT NOT NULL, nivel TEXT NOT NULL,
           bytes_enviados INTEGER NOT NULL, tokens_entrada INTEGER NOT NULL DEFAULT 0,
           tokens_saida INTEGER NOT NULL DEFAULT 0,
           custo_dezmilesimos_usd INTEGER NOT NULL DEFAULT 0,
           nominal INTEGER NOT NULL DEFAULT 0, quando TEXT NOT NULL, rodada_id INTEGER);",
    )
    .expect("tabela saida_ia");
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_ia::RegistroFalso;

    #[test]
    fn sem_inventario_o_registro_nao_abre_e_o_erro_diz_que_nada_foi_enviado() {
        let d = tempfile::tempdir().expect("pasta");
        let e = abrir_registro(d.path()).expect_err("sem inventário");
        assert!(e.to_string().contains("nada foi enviado"), "{e}");
        assert!(
            !d.path().join(ARQUIVO_DO_INVENTARIO).exists(),
            "não cria o inventário"
        );
    }

    #[test]
    fn inventario_sem_a_tabela_tambem_recusa() {
        let d = tempfile::tempdir().expect("pasta");
        rusqlite::Connection::open(d.path().join(ARQUIVO_DO_INVENTARIO)).expect("banco");
        let r = abrir_registro(d.path()).expect("abre");
        let e = registrar_antes(&r, "local", "m", 10).expect_err("sem tabela");
        assert!(e.to_string().contains("nada foi enviado"), "{e}");
    }

    #[test]
    fn registrar_antes_grava_rita_completo_e_completar_poe_os_tokens() {
        let d = tempfile::tempdir().expect("pasta");
        inventario_minimo_para_testes(d.path());
        let r = abrir_registro(d.path()).expect("abre");
        let id = registrar_antes(&r, "local", "qwen", 123).expect("registra");
        completar(
            &r,
            id,
            Consumo {
                tokens_entrada: 50,
                tokens_saida: 5,
                ..Consumo::default()
            },
        );
        let c = rusqlite::Connection::open(d.path().join(ARQUIVO_DO_INVENTARIO)).expect("banco");
        let linha: String = c
            .query_row(
                "SELECT destinatario||'|'||nivel||'|'||provedor||'|'||bytes_enviados||'|'||tokens_entrada FROM saida_ia",
                [],
                |l| l.get(0),
            )
            .expect("linha");
        assert_eq!(linha, "rita|completo|local|123|50");
    }

    #[test]
    fn registro_que_falha_nao_autoriza() {
        let f = RegistroFalso::que_falha();
        assert!(registrar_antes(&f, "x", "m", 1).is_err());
    }
}
