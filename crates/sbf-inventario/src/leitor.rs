//! O `LeitorSqlite`: a implementação de [`Consulta`] sobre o `inventario.sqlite`.
//!
//! - **Só leitura:** abre o banco numa conexão própria, com `SQLITE_OPEN_READ_ONLY` e
//!   `PRAGMA query_only`. Nada aqui escreve. A escrita é do núcleo (`Comandos`), em outra
//!   conexão; com WAL os dois convivem (LICOES.md), e o `busy_timeout` de 30 s cobre o
//!   checkpoint.
//! - **Tolerante:** `contraparte`, `data_compra` e `fatura_ref` são colunas que só o
//!   `aplicarSchema` do Bun acrescenta ([`Dialeto`]); sem elas leem `NULL`.
//! - **Relógio:** o ciclo atual vem de [`hoje_iso`] (America/Sao_Paulo); os testes fixam o
//!   dia com [`LeitorSqlite::com_hoje`].
//!
//! ```no_run
//! use sbf_inventario::consulta::Consulta;
//! use sbf_inventario::leitor::LeitorSqlite;
//!
//! let leitor = LeitorSqlite::abrir(std::path::Path::new("inventario.sqlite"))?;
//! let contagens = leitor.contagens()?;
//! # Ok::<(), sbf_inventario::consulta::ErroDeConsulta>(())
//! ```
//!
//! Dono: agente de leitura.

mod itens;

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::consulta::{
    Area, Consulta, Contagens, ErroDeConsulta, Filtro, Fonte, Item, Painel, PedidoDeTabela,
    Pergunta, Rastro, ResultadoDeConsulta, Tabela, TotalDoCiclo,
};
use crate::referencia::{Ref, Tipo};
use crate::relacoes::{self, lotes, pendencias, tabela_lotes, tabela_pendencias};
use crate::visao_geral::{self, EstadoManual, ciclo_da_data, hoje_iso};
use crate::vistas::{
    self, Contexto, Dialeto, Selecao, consultar, fim_do_ciclo, inicio_do_ciclo, texto,
};
use crate::{rastro, relatorios};

/// O leitor de só leitura do inventário.
#[derive(Debug)]
pub struct LeitorSqlite {
    conn: Connection,
    dialeto: Dialeto,
    hoje: Option<String>,
    manual: EstadoManual,
}

impl LeitorSqlite {
    /// Abre o banco só para leitura, numa conexão própria.
    pub fn abrir(caminho: &Path) -> ResultadoDeConsulta<Self> {
        let conn = Connection::open_with_flags(
            caminho,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::de_conexao(conn)
    }

    /// Usa uma conexão que o chamador abriu (por exemplo, em memória, nos testes). A conexão
    /// passa a ser só de leitura (`PRAGMA query_only`).
    pub fn de_conexao(conn: Connection) -> ResultadoDeConsulta<Self> {
        conn.busy_timeout(Duration::from_secs(30))?;
        conn.pragma_update(None, "query_only", true)?;
        let dialeto = Dialeto::detectar(&conn)?;
        Ok(Self {
            conn,
            dialeto,
            hoje: None,
            manual: EstadoManual::default(),
        })
    }

    /// Fixa o dia de hoje (`AAAA-MM-DD`), para os testes e para a janela reproduzir um dia.
    #[must_use]
    pub fn com_hoje(mut self, hoje: &str) -> Self {
        self.hoje = Some(hoje.to_owned());
        self
    }

    /// O que o banco não sabe: etapas abertas ou fechadas e avisos (`painel/estado.json`).
    #[must_use]
    pub fn com_estado_manual(mut self, manual: EstadoManual) -> Self {
        self.manual = manual;
        self
    }

    /// Relê as colunas opcionais do banco (depois de uma migração).
    pub fn reler_esquema(&mut self) -> ResultadoDeConsulta<()> {
        self.dialeto = Dialeto::detectar(&self.conn)?;
        Ok(())
    }

    /// O dia de hoje que o leitor usa.
    pub fn hoje(&self) -> String {
        self.hoje.clone().unwrap_or_else(hoje_iso)
    }

    /// O ciclo atual (`AAAA-MM`).
    pub fn ciclo_atual(&self) -> String {
        ciclo_da_data(&self.hoje()).unwrap_or_default()
    }

    fn ctx(&self) -> Contexto<'_> {
        Contexto {
            conn: &self.conn,
            dialeto: self.dialeto,
        }
    }

    /// A vista "E-mails por remetente", a outra metade da área E-mails.
    ///
    /// A área devolve as mensagens; esta devolve os domínios. A janela escolhe qual mostrar
    /// (sugestão para a costura: `Area::EmailsPorRemetente`).
    pub fn emails_por_remetente(&self, filtro: &Filtro) -> ResultadoDeConsulta<Tabela> {
        vistas::tabela_de_emails_por_remetente(&self.ctx(), filtro)
    }

    /// A vista "Pastas pessoais", a outra metade da área Arquivos e pastas.
    pub fn pastas_pessoais(&self, filtro: &Filtro) -> ResultadoDeConsulta<Tabela> {
        vistas::tabela_de_pastas_pessoais(&self.ctx(), filtro)
    }

    /// As regras de senha (nunca a senha) como tabela, para a janela de Ferramentas.
    pub fn regras_de_senha(&self) -> ResultadoDeConsulta<Tabela> {
        let ctx = self.ctx();
        Ok(relacoes::tabela_regras_de_senha(
            &relacoes::regras_de_senha(&ctx, &Selecao::nova())?,
        ))
    }

    /// Todas as conexões (fontes) como tabela.
    pub fn conexoes(&self) -> ResultadoDeConsulta<Tabela> {
        let ctx = self.ctx();
        Ok(relacoes::tabela_conexoes(&relacoes::conexoes(
            &ctx,
            &Selecao::nova(),
        )?))
    }

    fn tabela_de_pendencias(&self, filtro: &Filtro) -> ResultadoDeConsulta<Tabela> {
        let ctx = self.ctx();
        let mut sel = Selecao::nova();
        if let Some(t) = filtro
            .texto
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            let p = texto(format!("%{t}%"));
            sel = sel.onde(
                "p.pergunta LIKE ? OR p.resposta LIKE ? OR p.etapa LIKE ? OR p.tipo LIKE ?",
                vec![p.clone(), p.clone(), p.clone(), p],
            );
        }
        if let Some((de, ate)) = &filtro.ciclos {
            sel = sel.onde(
                "substr(p.criada_em, 1, 10) BETWEEN ? AND ?",
                vec![texto(inicio_do_ciclo(de)), texto(fim_do_ciclo(ate))],
            );
        }
        let abertas = filtro.estados.iter().any(|e| e == "aberta");
        let respondidas = filtro.estados.iter().any(|e| e == "respondida");
        if abertas && !respondidas {
            sel = sel.onde("p.resolvida_em IS NULL", vec![]);
        } else if respondidas && !abertas {
            sel = sel.onde("p.resolvida_em IS NOT NULL", vec![]);
        }
        tabela_pendencias(&ctx, &pendencias(&ctx, &sel)?)
    }

    fn tabela_de_lotes(&self, filtro: &Filtro) -> ResultadoDeConsulta<Tabela> {
        let ctx = self.ctx();
        let mut sel = Selecao::nova();
        if let Some(t) = filtro
            .texto
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            sel = sel.onde("l.nome LIKE ?", vec![texto(format!("%{t}%"))]);
        }
        if let Some((de, ate)) = &filtro.ciclos {
            sel = sel.onde(
                "substr(l.criado_em, 1, 10) BETWEEN ? AND ?",
                vec![texto(inicio_do_ciclo(de)), texto(fim_do_ciclo(ate))],
            );
        }
        let situacoes: Vec<&str> = filtro
            .estados
            .iter()
            .map(String::as_str)
            .filter(|e| matches!(*e, "aberto" | "aprovado" | "promovido"))
            .collect();
        if !situacoes.is_empty() {
            let clausulas: Vec<&str> = situacoes
                .iter()
                .map(|s| match *s {
                    "promovido" => "l.promovido_em IS NOT NULL",
                    "aprovado" => "(l.aprovado_em IS NOT NULL AND l.promovido_em IS NULL)",
                    _ => "(l.aprovado_em IS NULL AND l.promovido_em IS NULL)",
                })
                .collect();
            sel = sel.onde(clausulas.join(" OR "), vec![]);
        }
        Ok(tabela_lotes(&lotes(&ctx, &sel)?))
    }
}

impl Consulta for LeitorSqlite {
    fn tabela(&self, pedido: &PedidoDeTabela) -> ResultadoDeConsulta<Tabela> {
        let ctx = self.ctx();
        match pedido {
            PedidoDeTabela::Area { area, filtro } => match area {
                Area::Pendencias => self.tabela_de_pendencias(filtro),
                Area::Lotes => self.tabela_de_lotes(filtro),
                Area::VisaoGeral => Err(ErroDeConsulta::NaoSuportado(
                    "a Visão geral é um painel: use `visao_geral()`".to_owned(),
                )),
                Area::Relatorios => Err(ErroDeConsulta::NaoSuportado(
                    "os Relatórios são painéis: use `relatorio()`".to_owned(),
                )),
                outra => vistas::tabela_da_area(&ctx, *outra, filtro),
            },
            PedidoDeTabela::Ligados { de, tipo } => relacoes::tabela_ligados(&ctx, de, *tipo),
            PedidoDeTabela::ItensDoTotal {
                de,
                fonte,
                ciclo,
                recorte,
            } => relacoes::itens_do_total(&ctx, de, *fonte, ciclo.as_deref(), recorte.as_deref()),
        }
    }

    fn item(&self, alvo: &Ref) -> ResultadoDeConsulta<Option<Item>> {
        itens::montar(&self.ctx(), alvo, &self.hoje())
    }

    fn rastro(&self, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
        rastro::rastro_de(&self.ctx(), alvo)
    }

    fn por_ciclo(
        &self,
        alvo: &Ref,
        fonte: Fonte,
        ciclos: usize,
    ) -> ResultadoDeConsulta<Vec<TotalDoCiclo>> {
        relacoes::por_ciclo_de(&self.ctx(), alvo, fonte, ciclos, &self.ciclo_atual())
    }

    fn visao_geral(&self) -> ResultadoDeConsulta<Painel> {
        let visao = visao_geral::medir(&self.ctx(), &self.hoje(), &self.manual)?;
        Ok(visao_geral::painel(&visao))
    }

    fn relatorio(
        &self,
        pergunta: Pergunta,
        fonte: Fonte,
        filtro: &Filtro,
    ) -> ResultadoDeConsulta<Painel> {
        relatorios::responder(&self.ctx(), pergunta, fonte, filtro, &self.hoje())
    }

    fn contagens(&self) -> ResultadoDeConsulta<Contagens> {
        let linhas = consultar(
            &self.conn,
            "SELECT count(*), coalesce(sum(estado = 'documento'), 0), coalesce(sum(estado = 'duvida'), 0),
                    coalesce(sum(estado = 'novo'), 0)
               FROM achado",
            &[],
            |l| {
                Ok(Contagens {
                    achados: l.get(0)?,
                    documentos: l.get(1)?,
                    em_duvida: l.get(2)?,
                    a_triar: l.get(3)?,
                })
            },
        )?;
        Ok(linhas.into_iter().next().unwrap_or_default())
    }

    fn texto(&self, achado: &Ref) -> ResultadoDeConsulta<Option<String>> {
        if !matches!(achado.tipo, Tipo::Achado | Tipo::Documento) {
            return Ok(None);
        }
        let linhas = consultar(
            &self.conn,
            "SELECT conteudo FROM texto WHERE content_hash = ?",
            &[texto(achado.chave.as_str())],
            |l| l.get::<_, String>(0),
        )?;
        Ok(linhas.into_iter().next())
    }
}
