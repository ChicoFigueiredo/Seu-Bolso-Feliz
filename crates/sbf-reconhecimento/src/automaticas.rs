//! A única escrita sem clique no planejamento: as ligações de alta confiança que o
//! reconhecimento acha ([`Sugestao::e_automatica`]).
//!
//! - o **vínculo** do documento à ocorrência (o contracheque conferido; um padrão
//!   consolidado), por [`Alteracao::VincularDocumento`], com `Autor::Regra`
//!   (`"contracheque"` ou `"<id>@<versão>"`);
//! - a **conciliação** das três provas do salário (`"tres-provas"`) e a de um padrão
//!   consolidado (`"<id>@<versão>"`), por [`Alteracao::Conciliar`] confirmada.
//!
//! Tudo o mais (usar as rubricas, descontos em folha, dar baixa, conciliação com menos de
//! três provas) espera o clique. Quem aplica: a janela, quando o cartão chega
//! ([`aplicar_automaticas`]), e o worker, sem janela, logo depois de gravar a linha da Caixa
//! de entrada ([`aplicar_automaticas_do_resultado`]). É idempotente, e nunca refaz o que o
//! usuário desfez (par de conciliação desfeito, documento desvinculado).

use std::path::Path;

use sbf_planejamento::{
    Alteracao, Autor, Banco, Conciliacao, ErroDoPlanejamento, Planejamento, PlanejamentoSqlite,
    SemSaldos,
};
use sbf_planilha::movimentos::MovimentosDaPluggy;

use crate::salario::{ja_vinculado, vinculado_por_regra, vinculo_desfeito};
use crate::{Resultado, Sugestao};

/// Uma ligação automática que está valendo depois da aplicação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aplicada {
    /// O índice da sugestão em [`Resultado::sugestoes`].
    pub indice: usize,
    /// O nome do autor gravado (`Autor::Regra`).
    pub autor: String,
    /// Gravada agora; `false` quando já estava feita (a aplicação é idempotente).
    pub nova: bool,
    /// A conciliação confirmada, quando a ligação é uma conciliação (a janela liga o registro
    /// do salário ao crédito com ela).
    pub conciliacao: Option<Conciliacao>,
}

/// O que a aplicação fez.
#[derive(Debug, Default)]
pub struct Saida {
    /// As ligações que estão valendo (novas ou já feitas), na ordem das sugestões.
    pub aplicadas: Vec<Aplicada>,
    /// As que não puderam ser gravadas: o índice da sugestão e o erro do planejamento.
    pub falhas: Vec<(usize, ErroDoPlanejamento)>,
}

impl Saida {
    /// Quantas ligações esta aplicação gravou de fato.
    #[must_use]
    pub fn novas(&self) -> usize {
        self.aplicadas.iter().filter(|a| a.nova).count()
    }
}

/// Aplica as sugestões automáticas do resultado no `plan` dado. Não registra nada no catálogo
/// de regras e não toca na Caixa de entrada.
pub fn aplicar_automaticas(plan: &mut dyn Planejamento, resultado: &Resultado) -> Saida {
    let mut saida = Saida::default();
    for (indice, sugestao) in resultado.sugestoes.iter().enumerate() {
        if !sugestao.e_automatica() {
            continue;
        }
        let feita = if let Sugestao::Vincular { .. } = sugestao {
            vincular(plan, resultado, indice, sugestao)
        } else {
            conciliar(plan, indice, sugestao)
        };
        match feita {
            Ok(Some(a)) => saida.aplicadas.push(a),
            Ok(None) => {}
            Err(e) => saida.falhas.push((indice, e)),
        }
    }
    saida
}

fn vincular(
    plan: &mut dyn Planejamento,
    resultado: &Resultado,
    indice: usize,
    sugestao: &Sugestao,
) -> Result<Option<Aplicada>, ErroDoPlanejamento> {
    let (Some(autor), Sugestao::Vincular { alvo, .. }) =
        (sugestao.autor_do_vinculo_automatico(), sugestao)
    else {
        return Ok(None);
    };
    let sha = &resultado.sha256;
    let erro = |e: crate::salario::ErroDoSalario| ErroDoPlanejamento::Banco(e.to_string());
    // O usuário já disse "não" a este vínculo: a regra não o refaz.
    if vinculo_desfeito(plan, alvo, sha).map_err(erro)? {
        return Ok(None);
    }
    let a = |nova| Aplicada {
        indice,
        autor: autor.clone(),
        nova,
        conciliacao: None,
    };
    if vinculado_por_regra(plan, alvo, sha).map_err(erro)? {
        return Ok(Some(a(false)));
    }
    // Ligado por outra mão (o usuário, o MCP): não é "automático", e não se duplica.
    if ja_vinculado(plan, alvo, sha).map_err(erro)? {
        return Ok(None);
    }
    for alteracao in sugestao.alteracoes(sha, &resultado.nome, 0, None) {
        plan.aplicar(
            alteracao,
            Autor::Regra {
                regra: autor.clone(),
            },
        )?;
    }
    Ok(Some(a(true)))
}

fn conciliar(
    plan: &mut dyn Planejamento,
    indice: usize,
    sugestao: &Sugestao,
) -> Result<Option<Aplicada>, ErroDoPlanejamento> {
    let Some((conciliacao, autor)) = sugestao.conciliacao_automatica() else {
        return Ok(None);
    };
    // O usuário já disse "não" a este par: a regra não o refaz.
    if plan.conciliacao_desfeita(&conciliacao.ocorrencia, &conciliacao.movimento)? {
        return Ok(None);
    }
    let confirmada = Conciliacao {
        confirmada: true,
        ..conciliacao.clone()
    };
    let ja = plan
        .conciliacoes_da_ocorrencia(&confirmada.ocorrencia)?
        .iter()
        .any(|g| g.conciliacao.confirmada && g.conciliacao.movimento == confirmada.movimento);
    if !ja {
        plan.aplicar(
            Alteracao::Conciliar {
                conciliacao: confirmada.clone(),
            },
            Autor::Regra {
                regra: autor.clone(),
            },
        )?;
    }
    Ok(Some(Aplicada {
        indice,
        autor,
        nova: !ja,
        conciliacao: Some(confirmada),
    }))
}

/// Aplica as sugestões automáticas do resultado no `planejamento.sqlite` do cofre, sem
/// janela e sem clique. Sem sugestão automática, ou sem `planejamento.sqlite`, não faz nada
/// (nem abre o banco).
///
/// # Errors
/// O motivo, genérico (sem valor nem nome de documento), quando o planejamento não abre. Uma
/// sugestão que não pôde ser gravada vai em [`Saida::falhas`].
pub fn aplicar_automaticas_do_resultado(
    cofre: &Path,
    resultado: &Resultado,
) -> Result<Saida, String> {
    if !resultado.sugestoes.iter().any(Sugestao::e_automatica) {
        return Ok(Saida::default());
    }
    let caminho = cofre.join("planejamento.sqlite");
    if !caminho.is_file() {
        return Ok(Saida::default());
    }
    let banco = Banco::abrir(&caminho).map_err(|_| "não deu para abrir o planejamento")?;
    let mut plan = PlanejamentoSqlite::novo(banco, Box::new(SemSaldos))
        .com_movimentos(Box::new(MovimentosDaPluggy::do_cofre(cofre)));
    Ok(aplicar_automaticas(&mut plan, resultado))
}
