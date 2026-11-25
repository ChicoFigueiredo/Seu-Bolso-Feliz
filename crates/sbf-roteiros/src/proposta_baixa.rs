//! A **proposta de Baixa forçada** (spec 18, decisão 7): o MCP e a IA só **propõem**; quem
//! confirma e executa é o usuário, na janela.
//!
//! Baixa forçada é baixar de novo um período mesmo que o conteúdo já esteja no inventário
//! (`CONTEXT.md`). Como ela passa por cima do descarte por conteúdo, a decisão é do usuário:
//! uma proposta é só uma linha em `proposta_baixa_forcada` (roteiro, período, autor, estado).
//! Ela aparece na janela como "Baixar de novo: <roteiro> <período>"; ao aceitar, a **janela**
//! executa o roteiro com `forcar = true` ([`BancoDeRoteiros::pedido_da_proposta_de_baixa`] monta
//! o pedido, [`BancoDeRoteiros::decidir_proposta_de_baixa`] marca o desfecho).
//!
//! Esta fronteira é **convenção verificada por teste** (`tests/autoridade.rs`): o MCP, o worker
//! e a IA não citam `pedido_da_proposta_de_baixa` nem `decidir_proposta_de_baixa`, e o MCP e a
//! IA também não citam as funções que executam roteiro escrito. Só [`BancoDeRoteiros::propor_baixa_forcada`]
//! é do MCP.
//!
//! A proposta vale 7 dias (como as do controle mensal) e só guarda mês e nome: nunca segredo.

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, params};

use crate::banco::{BancoDeRoteiros, EstadoEscrito};
use crate::erro::{ErroDeRoteiro, Resultado};
use crate::manifesto::Manifesto;
use crate::parametros::{EspecDeMes, ValorBruto};
use crate::servico::{PedidoDeExecucao, resolver_pedido};

/// Quanto tempo uma proposta espera o usuário: 7 dias.
pub const VALIDADE_EM_SEGUNDOS: i64 = 7 * 24 * 3600;
/// Quantas propostas pendentes (não vencidas) o banco aceita de uma vez.
pub const MAXIMO_DE_PENDENTES: usize = 20;

pub(crate) const SQL: &str = "
CREATE TABLE IF NOT EXISTS proposta_baixa_forcada (
  id            INTEGER PRIMARY KEY,
  roteiro_id    INTEGER NOT NULL REFERENCES roteiro_escrito(id),
  de            TEXT,
  ate           TEXT,
  autor         TEXT NOT NULL,
  criada_unix   INTEGER NOT NULL,
  estado        TEXT NOT NULL CHECK (estado IN ('pendente','aceita','recusada')),
  decidida_unix INTEGER
);
CREATE INDEX IF NOT EXISTS proposta_baixa_forcada_pendentes
  ON proposta_baixa_forcada(estado, criada_unix);
";

/// Onde a proposta está.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoDaPropostaDeBaixa {
    /// Espera o usuário.
    Pendente,
    /// O usuário aceitou e a janela executou.
    Aceita,
    /// O usuário recusou.
    Recusada,
}

impl EstadoDaPropostaDeBaixa {
    fn de(t: &str) -> Self {
        match t {
            "aceita" => Self::Aceita,
            "recusada" => Self::Recusada,
            _ => Self::Pendente,
        }
    }

    /// O texto que o banco guarda.
    #[must_use]
    pub const fn texto(self) -> &'static str {
        match self {
            Self::Pendente => "pendente",
            Self::Aceita => "aceita",
            Self::Recusada => "recusada",
        }
    }
}

/// Uma proposta de Baixa forçada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropostaDeBaixa {
    pub id: i64,
    pub roteiro: String,
    /// O `de` como foi proposto (`AAAA-MM` ou relativo); `None` = o padrão do roteiro.
    pub de: Option<String>,
    /// O `ate`, idem.
    pub ate: Option<String>,
    /// Quem propôs (`mcp`).
    pub autor: String,
    pub criada_unix: i64,
    pub estado: EstadoDaPropostaDeBaixa,
}

impl PropostaDeBaixa {
    /// O período em português, para a janela: `2026-10 a 2026-11`, `a partir de -1`,
    /// `até 0` ou `período padrão do roteiro`.
    #[must_use]
    pub fn periodo(&self) -> String {
        match (&self.de, &self.ate) {
            (Some(d), Some(a)) if d == a => format!("mês {d}"),
            (Some(d), Some(a)) => format!("{d} a {a}"),
            (Some(d), None) => format!("a partir de {d}"),
            (None, Some(a)) => format!("até {a}"),
            (None, None) => "período padrão do roteiro".to_owned(),
        }
    }

    /// O título que a janela mostra: `Baixar de novo: <roteiro> <período>`.
    #[must_use]
    pub fn titulo(&self) -> String {
        format!("Baixar de novo: {} {}", self.roteiro, self.periodo())
    }
}

/// O pedido de execução de um período, com a Baixa forçada se `forcar`: o que a janela monta
/// do formulário e da proposta aceita. Valores brutos (o mês relativo ainda não foi resolvido).
#[must_use]
pub fn pedido_de_periodo(de: Option<&str>, ate: Option<&str>, forcar: bool) -> PedidoDeExecucao {
    let mut entregues = BTreeMap::new();
    if let Some(d) = de {
        entregues.insert("de".to_owned(), ValorBruto::Texto(d.to_owned()));
    }
    if let Some(a) = ate {
        entregues.insert("ate".to_owned(), ValorBruto::Texto(a.to_owned()));
    }
    PedidoDeExecucao {
        entregues,
        forcar,
        entrada: None,
        previsto_unix: None,
    }
}

/// Confere um mês digitado (`AAAA-MM` ou relativo: `0`, `-1`…), pela mesma regra do manifesto.
/// Devolve o texto aparado, ou `None` se estava vazio.
///
/// # Errors
/// [`ErroDeRoteiro::Invalido`] com o motivo, em português.
pub fn conferir_mes(nome: &str, texto: &str) -> Resultado<Option<String>> {
    let t = texto.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if EspecDeMes::ler(t).is_none() {
        return Err(ErroDeRoteiro::Invalido(format!(
            "`{nome}`: use AAAA-MM (como 2026-10) ou relativo ao mês corrente (0, -1, -2…)"
        )));
    }
    Ok(Some(t.to_owned()))
}

/// O manifesto contra o qual se confere um período: o da versão ativa e, sem ela, o da mais
/// nova.
fn manifesto_de_referencia(banco: &BancoDeRoteiros, nome: &str) -> Resultado<Manifesto> {
    let d = banco.mostrar(nome)?;
    let escolhida = d
        .versoes
        .iter()
        .find(|v| v.estado == EstadoEscrito::Ativo)
        .or_else(|| d.versoes.iter().max_by_key(|v| v.numero))
        .ok_or_else(|| {
            ErroDeRoteiro::Invalido(format!("roteiro escrito `{nome}` não tem versões"))
        })?;
    Ok(escolhida.manifesto.clone())
}

type Linha = (
    i64,
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
    String,
);

fn montar(l: Linha) -> PropostaDeBaixa {
    PropostaDeBaixa {
        id: l.0,
        roteiro: l.1,
        de: l.2,
        ate: l.3,
        autor: l.4,
        criada_unix: l.5,
        estado: EstadoDaPropostaDeBaixa::de(&l.6),
    }
}

const COLUNAS: &str = "p.id, r.nome, p.de, p.ate, p.autor, p.criada_unix, p.estado";

pub(crate) fn preparar(conn: &rusqlite::Connection) -> Resultado<()> {
    conn.execute_batch(SQL)?;
    Ok(())
}

impl BancoDeRoteiros {
    /// Confere um pedido de Baixa forçada **sem gravar nada**: o roteiro existe, os meses seguem
    /// a regra do manifesto e o período passa na mesma conta da execução (o manifesto declara
    /// `de`/`ate`, `de` não é depois de `ate`, no máximo 60 meses). Devolve o período já aparado.
    ///
    /// # Errors
    /// As de [`Self::propor_baixa_forcada`], menos o teto de pendentes.
    pub fn validar_baixa_forcada(
        &self,
        nome: &str,
        de: Option<&str>,
        ate: Option<&str>,
        agora_unix: i64,
    ) -> Resultado<(Option<String>, Option<String>)> {
        let de = match de {
            Some(t) => conferir_mes("de", t)?,
            None => None,
        };
        let ate = match ate {
            Some(t) => conferir_mes("ate", t)?,
            None => None,
        };
        let existe: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM roteiro_escrito WHERE nome = ?1",
                [nome],
                |l| l.get(0),
            )
            .optional()?;
        if existe.is_none() {
            return Err(ErroDeRoteiro::Invalido(format!(
                "roteiro escrito `{nome}` não encontrado"
            )));
        }
        let manifesto = manifesto_de_referencia(self, nome)?;
        resolver_pedido(
            &manifesto,
            &pedido_de_periodo(de.as_deref(), ate.as_deref(), true),
            agora_unix,
        )?;
        Ok((de, ate))
    }

    /// **Propõe** uma Baixa forçada de `nome` no período `de` a `ate` (cada um `AAAA-MM` ou
    /// relativo; sem eles, o padrão do roteiro). Só grava a proposta: não executa nada, não
    /// força nada. Devolve a proposta e se ela é nova (a mesma proposta pendente, com o mesmo
    /// período, não duplica).
    ///
    /// # Errors
    /// Roteiro inexistente, mês fora da regra do manifesto, período que o manifesto não declara
    /// (`de`/`ate`), `de` depois de `ate`, mais de 60 meses, pendentes demais ou falha do banco.
    pub fn propor_baixa_forcada(
        &self,
        nome: &str,
        de: Option<&str>,
        ate: Option<&str>,
        autor: &str,
        agora_unix: i64,
    ) -> Resultado<(PropostaDeBaixa, bool)> {
        let (de, ate) = self.validar_baixa_forcada(nome, de, ate, agora_unix)?;
        let roteiro_id: i64 = self.conn.query_row(
            "SELECT id FROM roteiro_escrito WHERE nome = ?1",
            [nome],
            |l| l.get(0),
        )?;
        let validas = agora_unix - VALIDADE_EM_SEGUNDOS;
        let existente = self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLUNAS} FROM proposta_baixa_forcada p
                       JOIN roteiro_escrito r ON r.id = p.roteiro_id
                      WHERE p.roteiro_id = ?1 AND p.estado = 'pendente' AND p.criada_unix > ?4
                        AND p.de IS ?2 AND p.ate IS ?3"
                ),
                params![roteiro_id, de, ate, validas],
                ler,
            )
            .optional()?;
        if let Some(l) = existente {
            return Ok((montar(l), false));
        }
        let pendentes: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM proposta_baixa_forcada
              WHERE estado = 'pendente' AND criada_unix > ?1",
            [validas],
            |l| l.get(0),
        )?;
        if usize::try_from(pendentes).unwrap_or(usize::MAX) >= MAXIMO_DE_PENDENTES {
            return Err(ErroDeRoteiro::Invalido(format!(
                "já há {MAXIMO_DE_PENDENTES} propostas de baixa pendentes: o usuário precisa decidir antes"
            )));
        }
        self.conn.execute(
            "INSERT INTO proposta_baixa_forcada (roteiro_id, de, ate, autor, criada_unix, estado)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pendente')",
            params![roteiro_id, de, ate, autor, agora_unix],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok((self.proposta_de_baixa(id)?, true))
    }

    /// Uma proposta pelo id.
    ///
    /// # Errors
    /// Proposta inexistente ou falha do banco.
    pub fn proposta_de_baixa(&self, id: i64) -> Resultado<PropostaDeBaixa> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {COLUNAS} FROM proposta_baixa_forcada p
                       JOIN roteiro_escrito r ON r.id = p.roteiro_id WHERE p.id = ?1"
                ),
                [id],
                ler,
            )
            .optional()?
            .map(montar)
            .ok_or_else(|| ErroDeRoteiro::Invalido(format!("proposta de baixa {id} não existe")))
    }

    /// As propostas **pendentes** e ainda dentro da validade, as mais novas primeiro.
    ///
    /// # Errors
    /// Falha do banco.
    pub fn propostas_de_baixa_pendentes(&self, agora_unix: i64) -> Resultado<Vec<PropostaDeBaixa>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLUNAS} FROM proposta_baixa_forcada p
               JOIN roteiro_escrito r ON r.id = p.roteiro_id
              WHERE p.estado = 'pendente' AND p.criada_unix > ?1
              ORDER BY p.criada_unix DESC, p.id DESC"
        ))?;
        let linhas = st
            .query_map([agora_unix - VALIDADE_EM_SEGUNDOS], ler)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(linhas.into_iter().map(montar).collect())
    }

    /// O pedido de execução de uma proposta pendente: o período dela com a **Baixa forçada**.
    /// Só a janela do usuário chama (`tests/autoridade.rs`); quem chama executa em seguida e
    /// então [`Self::decidir_proposta_de_baixa`].
    ///
    /// # Errors
    /// Proposta inexistente, já decidida ou vencida.
    pub fn pedido_da_proposta_de_baixa(
        &self,
        id: i64,
        agora_unix: i64,
    ) -> Resultado<PedidoDeExecucao> {
        let p = self.proposta_de_baixa(id)?;
        if p.estado != EstadoDaPropostaDeBaixa::Pendente {
            return Err(ErroDeRoteiro::Invalido(
                "a proposta de baixa já foi decidida".into(),
            ));
        }
        if p.criada_unix <= agora_unix - VALIDADE_EM_SEGUNDOS {
            return Err(ErroDeRoteiro::Invalido(
                "a proposta de baixa passou da validade de 7 dias".into(),
            ));
        }
        Ok(pedido_de_periodo(p.de.as_deref(), p.ate.as_deref(), true))
    }

    /// Marca o desfecho de uma proposta pendente: aceita (a janela executou) ou recusada. Só a
    /// janela do usuário chama.
    ///
    /// # Errors
    /// Proposta inexistente ou já decidida.
    pub fn decidir_proposta_de_baixa(
        &self,
        id: i64,
        aceita: bool,
        agora_unix: i64,
    ) -> Resultado<()> {
        let estado = if aceita { "aceita" } else { "recusada" };
        let alterou = self.conn.execute(
            "UPDATE proposta_baixa_forcada SET estado = ?2, decidida_unix = ?3
              WHERE id = ?1 AND estado = 'pendente'",
            params![id, estado, agora_unix],
        )?;
        if alterou == 0 {
            // Distingue "não existe" de "já decidida".
            self.proposta_de_baixa(id)?;
            return Err(ErroDeRoteiro::Invalido(
                "a proposta de baixa já foi decidida".into(),
            ));
        }
        Ok(())
    }
}

fn ler(l: &rusqlite::Row<'_>) -> rusqlite::Result<Linha> {
    Ok((
        l.get(0)?,
        l.get(1)?,
        l.get(2)?,
        l.get(3)?,
        l.get(4)?,
        l.get(5)?,
        l.get(6)?,
    ))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::banco::{AutorEscrito, Confirmacao, ResultadoDoTeste};

    const AGORA: i64 = 1_790_000_000;

    fn manifesto(nome: &str, com_periodo: bool) -> Manifesto {
        let parametros = if com_periodo {
            r#","parametros":[{"nome":"de","tipo":"mes","padrao":"0"},{"nome":"ate","tipo":"mes","padrao":"0"}]"#
        } else {
            ""
        };
        Manifesto::de_json(&format!(
            r#"{{"nome":"{nome}","descricao":"fictício","url_inicial":"https://portal.exemplo.test/entrar",
            "hosts":["portal.exemplo.test"],"rotulo_senha":null,"rotulo_cpf":null,
            "mfa":{{"tipo":"nenhum"}},"movimenta_dinheiro":false,"classe":"consulta"{parametros}}}"#
        ))
        .unwrap()
    }

    fn banco(nome: &str, com_periodo: bool, ativar: bool) -> BancoDeRoteiros {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let m = manifesto(nome, com_periodo);
        let n = b
            .importar_escrito(&m, "navegador.registrar('oi')", &AutorEscrito::Usuario, "t")
            .unwrap()
            .numero;
        if ativar {
            b.marcar_testado(
                nome,
                n,
                &ResultadoDoTeste {
                    passou: true,
                    resumo: "verde".into(),
                },
            )
            .unwrap();
            let d = Confirmacao::texto_de_ativar(nome, n);
            let c = Confirmacao::do_terminal(true, &d, &d).unwrap();
            b.ativar(nome, n, &c).unwrap();
        }
        b
    }

    #[test]
    fn propoe_e_lista_como_pendente_sem_executar_nada() {
        let b = banco("contracheque-exemplo", true, true);
        let (p, nova) = b
            .propor_baixa_forcada("contracheque-exemplo", Some("-1"), Some("0"), "mcp", AGORA)
            .unwrap();
        assert!(nova);
        assert_eq!(p.estado, EstadoDaPropostaDeBaixa::Pendente);
        assert_eq!(p.autor, "mcp");
        assert_eq!(p.titulo(), "Baixar de novo: contracheque-exemplo -1 a 0");
        let pendentes = b.propostas_de_baixa_pendentes(AGORA + 5).unwrap();
        assert_eq!(pendentes, vec![p]);
        // não criou execução nenhuma
        assert!(b.historico_escrito(None, 10).unwrap().is_empty());
    }

    #[test]
    fn a_mesma_proposta_pendente_nao_duplica() {
        let b = banco("c", true, true);
        let (a, nova_a) = b
            .propor_baixa_forcada("c", Some("2026-10"), Some("2026-11"), "mcp", AGORA)
            .unwrap();
        let (c, nova_c) = b
            .propor_baixa_forcada("c", Some("2026-10"), Some("2026-11"), "mcp", AGORA + 60)
            .unwrap();
        assert!(nova_a && !nova_c);
        assert_eq!(a.id, c.id);
        // outro período é outra proposta
        let (d, nova_d) = b
            .propor_baixa_forcada("c", Some("-3"), None, "mcp", AGORA)
            .unwrap();
        assert!(nova_d);
        assert_ne!(d.id, a.id);
        assert_eq!(b.propostas_de_baixa_pendentes(AGORA).unwrap().len(), 2);
    }

    #[test]
    fn periodo_invalido_roteiro_inexistente_e_parametro_nao_declarado_sao_recusados() {
        let b = banco("c", true, true);
        let erro = |de: Option<&str>, ate: Option<&str>, nome: &str| {
            b.propor_baixa_forcada(nome, de, ate, "mcp", AGORA)
                .unwrap_err()
                .to_string()
        };
        assert!(erro(Some("2026-13"), None, "c").contains("AAAA-MM"));
        assert!(erro(Some("+1"), None, "c").contains("AAAA-MM"));
        assert!(erro(Some("2026-11"), Some("2026-10"), "c").contains("depois de"));
        assert!(erro(Some("2020-01"), Some("2026-11"), "c").contains("60 meses"));
        assert!(erro(None, None, "nao-existe").contains("não encontrado"));
        // sem período declarado no manifesto, entregar `de` é erro; sem entregar, passa
        let s = banco("s", false, true);
        assert!(
            s.propor_baixa_forcada("s", Some("0"), None, "mcp", AGORA)
                .unwrap_err()
                .to_string()
                .contains("não declara")
        );
        assert!(
            s.propor_baixa_forcada("s", None, None, "mcp", AGORA)
                .is_ok()
        );
        assert!(b.propostas_de_baixa_pendentes(AGORA).unwrap().is_empty());
    }

    #[test]
    fn confere_contra_o_rascunho_quando_nao_ha_versao_ativa() {
        let b = banco("r", true, false);
        assert!(
            b.propor_baixa_forcada("r", Some("-1"), Some("0"), "mcp", AGORA)
                .is_ok()
        );
    }

    #[test]
    fn aceitar_monta_o_pedido_forcado_e_decide_uma_vez_so() {
        let b = banco("c", true, true);
        let (p, _) = b
            .propor_baixa_forcada("c", Some("2026-10"), Some("2026-11"), "mcp", AGORA)
            .unwrap();
        let pedido = b.pedido_da_proposta_de_baixa(p.id, AGORA + 1).unwrap();
        assert!(pedido.forcar);
        assert_eq!(
            pedido.entregues.get("de"),
            Some(&ValorBruto::Texto("2026-10".into()))
        );
        assert_eq!(
            pedido.entregues.get("ate"),
            Some(&ValorBruto::Texto("2026-11".into()))
        );
        b.decidir_proposta_de_baixa(p.id, true, AGORA + 2).unwrap();
        assert_eq!(
            b.proposta_de_baixa(p.id).unwrap().estado,
            EstadoDaPropostaDeBaixa::Aceita
        );
        assert!(
            b.propostas_de_baixa_pendentes(AGORA + 3)
                .unwrap()
                .is_empty()
        );
        assert!(b.pedido_da_proposta_de_baixa(p.id, AGORA + 3).is_err());
        assert!(b.decidir_proposta_de_baixa(p.id, false, AGORA + 3).is_err());
        assert!(b.decidir_proposta_de_baixa(999, false, AGORA).is_err());
    }

    #[test]
    fn recusar_e_vencer() {
        let b = banco("c", true, true);
        let (p, _) = b
            .propor_baixa_forcada("c", None, None, "mcp", AGORA)
            .unwrap();
        assert_eq!(p.titulo(), "Baixar de novo: c período padrão do roteiro");
        b.decidir_proposta_de_baixa(p.id, false, AGORA + 1).unwrap();
        assert_eq!(
            b.proposta_de_baixa(p.id).unwrap().estado,
            EstadoDaPropostaDeBaixa::Recusada
        );
        let (q, _) = b
            .propor_baixa_forcada("c", Some("0"), None, "mcp", AGORA)
            .unwrap();
        let depois = AGORA + VALIDADE_EM_SEGUNDOS + 1;
        assert!(b.propostas_de_baixa_pendentes(depois).unwrap().is_empty());
        assert!(
            b.pedido_da_proposta_de_baixa(q.id, depois)
                .unwrap_err()
                .to_string()
                .contains("validade")
        );
    }

    #[test]
    fn ha_um_teto_de_pendentes() {
        let b = banco("c", true, true);
        for i in 0..MAXIMO_DE_PENDENTES {
            b.propor_baixa_forcada("c", Some(&format!("-{}", i + 1)), None, "mcp", AGORA)
                .unwrap();
        }
        let e = b
            .propor_baixa_forcada("c", Some("2026-01"), None, "mcp", AGORA)
            .unwrap_err();
        assert!(e.to_string().contains("pendentes"), "{e}");
    }

    #[test]
    fn titulos_do_periodo() {
        let p = |de: Option<&str>, ate: Option<&str>| PropostaDeBaixa {
            id: 1,
            roteiro: "r".into(),
            de: de.map(str::to_owned),
            ate: ate.map(str::to_owned),
            autor: "mcp".into(),
            criada_unix: 0,
            estado: EstadoDaPropostaDeBaixa::Pendente,
        };
        assert_eq!(p(Some("-1"), Some("-1")).periodo(), "mês -1");
        assert_eq!(p(Some("-1"), None).periodo(), "a partir de -1");
        assert_eq!(p(None, Some("0")).periodo(), "até 0");
    }

    #[test]
    fn conferir_mes_usa_a_regra_do_manifesto() {
        assert_eq!(conferir_mes("de", "  ").unwrap(), None);
        assert_eq!(
            conferir_mes("de", " 2026-10 ").unwrap().as_deref(),
            Some("2026-10")
        );
        assert_eq!(conferir_mes("de", "-12").unwrap().as_deref(), Some("-12"));
        for ruim in ["2026-00", "2026-1", "1", "-01", "abc", "2026/10"] {
            assert!(conferir_mes("de", ruim).is_err(), "{ruim}");
        }
    }
}
