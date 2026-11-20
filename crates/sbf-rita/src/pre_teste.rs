//! O pré-teste (spec 15, D15): roda um rascunho sobre o item, sobre os exemplos e sobre os
//! documentos parecidos do cofre que têm texto guardado. Só leitura: nada é gravado.
//!
//! Mostra se o item casa e os campos tirados, quantos parecidos casariam (com até 10 títulos),
//! com quais versões ativas o rascunho empataria, de quais ele derrubaria documentos e quantos
//! documentos hoje sem padrão ele passaria a reconhecer. O diff vem de
//! [`sbf_regras::simulacao`].

use std::sync::Arc;

use sbf_regras::catalogo::{Catalogo, VersaoDeRegra};
use sbf_regras::motor::Motor;
use sbf_regras::simulacao::{ResumoDaDecisao, simular};
use sbf_regras::{Decisao, Entrada, Leitura, ResultadoDaExecucao};
use serde::{Deserialize, Serialize};

use crate::contexto::Contexto;
use crate::diagnostico::diagnosticar;
use crate::texto::cortar;

/// Quantos documentos parecidos o pré-teste examina, no máximo.
pub const MAXIMO_DE_PARECIDOS: usize = 150;
/// Quantos títulos de documentos que casariam aparecem.
pub const MAXIMO_DE_TITULOS: usize = 10;

/// O que o motor decidiu para o item com o rascunho em jogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisaoNoItem {
    /// O rascunho venceu.
    Esta,
    /// Outro padrão venceu (o id).
    Outra(String),
    /// Empate com estes padrões, em desacordo: vai para revisão humana.
    Ambigua(Vec<String>),
    Nenhuma,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoItem {
    pub casa: bool,
    pub decisao: DecisaoNoItem,
    /// `nome = valor` dos campos tirados (só quando casa).
    pub campos: Vec<(String, String)>,
    /// Por que não casou ou falhou (curto): a condição ou o campo, o trecho que falta e as
    /// linhas do texto que se parecem.
    pub diagnostico: Vec<String>,
    /// Quantas condições de `detectar` não se cumprem.
    #[serde(default)]
    pub condicoes_falhando: usize,
    /// Os campos declarados (`extrair`, `obrigatorios`) que não saíram do item.
    #[serde(default)]
    pub faltam: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DosExemplos {
    pub total: usize,
    pub passaram: usize,
    pub falharam: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaAmostra {
    pub examinados: usize,
    pub casam: usize,
    /// Até [`MAXIMO_DE_TITULOS`].
    pub titulos: Vec<String>,
}

/// Um padrão ativo e quantos documentos do pré-teste ele afeta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Afetado {
    pub regra: String,
    pub quantos: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreTeste {
    pub regra: String,
    pub versao: u32,
    pub item: NoItem,
    pub exemplos: DosExemplos,
    pub amostra: DaAmostra,
    /// Padrões ativos com que o rascunho empataria, em desacordo.
    pub empates: Vec<Afetado>,
    /// Padrões ativos de que o rascunho tomaria documentos.
    pub derrubadas: Vec<Afetado>,
    /// Documentos que hoje nenhum padrão reconhece e o rascunho passaria a reconhecer.
    pub novos: usize,
    /// Documentos que esta regra reconhece hoje e o rascunho deixaria de reconhecer.
    pub perdem: usize,
    pub passou: bool,
    /// Por que não passou (vazio quando passou).
    pub motivo: String,
}

impl PreTeste {
    /// O pré-teste mostrou empate ou que o rascunho derruba documentos de outro padrão: o botão
    /// Ativar pede confirmação (D10).
    pub fn pede_confirmacao(&self) -> bool {
        !self.empates.is_empty()
            || !self.derrubadas.is_empty()
            || matches!(self.item.decisao, DecisaoNoItem::Ambigua(_))
            || self.perdem > 0
    }

    /// As linhas que o cartão do rascunho e a CLI mostram.
    pub fn linhas(&self) -> Vec<String> {
        let mut l = Vec::new();
        l.push(if self.item.casa {
            let campos = self
                .item
                .campos
                .iter()
                .map(|(n, v)| format!("{n} = {v}"))
                .collect::<Vec<_>>()
                .join("; ");
            let falhas = if self.item.diagnostico.is_empty() {
                String::new()
            } else {
                format!(" {}", self.item.diagnostico.join(" "))
            };
            format!("Este item: casa. {campos}{falhas}")
        } else {
            format!("Este item: não casa. {}", self.item.diagnostico.join(" "))
        });
        if let DecisaoNoItem::Outra(r) = &self.item.decisao {
            l.push(format!("Neste item o padrão `{r}` venceria."));
        }
        if let DecisaoNoItem::Ambigua(rs) = &self.item.decisao {
            l.push(format!("Neste item haveria empate com: {}.", rs.join(", ")));
        }
        if self.exemplos.total > 0 {
            l.push(format!(
                "Exemplos: {} de {} passam{}.",
                self.exemplos.passaram,
                self.exemplos.total,
                if self.exemplos.falharam.is_empty() {
                    String::new()
                } else {
                    format!(" (falham: {})", self.exemplos.falharam.join(", "))
                }
            ));
        }
        l.push(format!(
            "No cofre: casam {} de {} documentos parecidos{}.",
            self.amostra.casam,
            self.amostra.examinados,
            if self.amostra.titulos.is_empty() {
                String::new()
            } else {
                format!(" ({})", self.amostra.titulos.join("; "))
            }
        ));
        for a in &self.empates {
            l.push(format!(
                "Empata com `{}` em {} documentos.",
                a.regra, a.quantos
            ));
        }
        for a in &self.derrubadas {
            l.push(format!(
                "Derruba `{}` em {} documentos.",
                a.regra, a.quantos
            ));
        }
        if self.novos > 0 {
            l.push(format!(
                "Reconhece {} que hoje nenhum padrão reconhece.",
                self.novos
            ));
        }
        if self.perdem > 0 {
            l.push(format!(
                "Deixa de reconhecer {} que a versão em vigor reconhece.",
                self.perdem
            ));
        }
        l
    }

    /// O resultado para o modelo: JSON compacto, só o que ele precisa para decidir o próximo
    /// passo.
    pub fn para_o_modelo(&self) -> String {
        self.para_o_modelo_json().to_string()
    }

    /// O mesmo resultado, como valor JSON (quem chama pode acrescentar chaves).
    pub fn para_o_modelo_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "regra": self.regra,
            "versao": self.versao,
            "passou": self.passou,
            "item": {
                "casa": self.item.casa,
                "decisao": self.item.decisao,
                "campos": self.item.campos.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
                "diagnostico": self.item.diagnostico,
            },
            "cofre": {
                "examinados": self.amostra.examinados,
                "casam": self.amostra.casam,
                "titulos": self.amostra.titulos,
            },
            "empates": self.empates,
            "derruba": self.derrubadas,
            "novos": self.novos,
            "perdem": self.perdem,
        });
        if self.exemplos.total > 0 {
            v["exemplos"] = serde_json::json!(format!(
                "{} de {} passam",
                self.exemplos.passaram, self.exemplos.total
            ));
        }
        if !self.passou {
            v["motivo"] = serde_json::json!(self.motivo);
        }
        v
    }
}

/// O título de uma entrada para listas: o nome do arquivo, ou `documento <8 do hash>`.
pub fn titulo_da_entrada(e: &Entrada) -> String {
    match e.nome.as_deref().filter(|n| !n.trim().is_empty()) {
        Some(n) => cortar(n, 60),
        None => format!("documento {}", e.id.chars().take(8).collect::<String>()),
    }
}

fn casou(l: &Leitura, regra: &str, versao: u32) -> bool {
    l.execucoes.iter().any(|e| {
        e.regra == regra && e.versao == versao && e.resultado == ResultadoDaExecucao::Reconhecido
    })
}

fn no_item(leitura: &Leitura, alvo: &VersaoDeRegra, item: &Entrada, pedido: &str) -> NoItem {
    let (regra, versao) = (alvo.regra.as_str(), alvo.versao);
    let casa = casou(leitura, regra, versao);
    let decisao = match &leitura.decisao {
        Decisao::Reconhecido(r) if r.regra == regra => DecisaoNoItem::Esta,
        Decisao::Reconhecido(r) => DecisaoNoItem::Outra(r.regra.clone()),
        Decisao::Ambiguo { candidatas, .. } => {
            DecisaoNoItem::Ambigua(candidatas.iter().map(|c| c.regra.clone()).collect())
        }
        Decisao::Nenhuma => DecisaoNoItem::Nenhuma,
    };
    let rec = match &leitura.decisao {
        Decisao::Reconhecido(r) if r.regra == regra => Some(r.as_ref()),
        Decisao::Ambiguo { candidatas, .. } => candidatas.iter().find(|c| c.regra == regra),
        _ => None,
    };
    let campos = rec
        .map(|r| {
            r.campos
                .iter()
                .map(|(n, c)| (n.clone(), cortar(&c.valor.to_string(), 60)))
                .collect()
        })
        .unwrap_or_default();
    let mut diagnostico = Vec::new();
    if !casa {
        if let Some(e) = leitura
            .execucoes
            .iter()
            .find(|e| e.regra == regra && e.versao == versao)
        {
            if e.resultado == ResultadoDaExecucao::Erro {
                diagnostico.push(format!(
                    "erro do padrão ({}): {}",
                    e.motivo.as_deref().unwrap_or("?"),
                    cortar(e.detalhe.as_deref().unwrap_or(""), 160)
                ));
            }
        } else {
            diagnostico.push("o tipo de entrada do item não está em `aplica_a`.".to_owned());
        }
    }
    let d = diagnosticar(&alvo.definicao, item, pedido);
    diagnostico.extend(d.linhas);
    if !casa && diagnostico.is_empty() {
        diagnostico.push("as condições `detectar` não se cumprem.".to_owned());
    }
    NoItem {
        casa,
        decisao,
        campos,
        diagnostico,
        condicoes_falhando: d.condicoes_falhando,
        faltam: d.faltam,
    }
}

/// Roda o pré-teste da versão `versao` da regra `regra` (qualquer estado) do catálogo.
///
/// # Errors
/// A versão não existe ou o catálogo não abre (texto curto, sem conteúdo).
pub fn pre_testar(
    ctx: &Contexto,
    catalogo: &Catalogo,
    regra: &str,
    versao: u32,
) -> Result<PreTeste, String> {
    let alvo = catalogo
        .versao(regra, versao)
        .map_err(|_| format!("não achei a versão {versao} do padrão `{regra}`"))?;
    pre_testar_versao(ctx, catalogo, &alvo, "")
}

/// O mesmo pré-teste sobre uma versão que pode nem estar gravada (um ensaio: nada é escrito).
/// `pedido` é o texto do usuário; as palavras dele ajudam o diagnóstico a achar as linhas
/// candidatas.
///
/// # Errors
/// O catálogo não abre ou não lê (texto curto, sem conteúdo).
pub fn pre_testar_versao(
    ctx: &Contexto,
    catalogo: &Catalogo,
    alvo: &VersaoDeRegra,
    pedido: &str,
) -> Result<PreTeste, String> {
    let (regra, versao) = (alvo.regra.as_str(), alvo.versao);
    let exec = Some(Arc::clone(&ctx.executor));
    let vigentes = catalogo
        .vigentes()
        .map_err(|_| "falha ao ler os padrões".to_owned())?;
    let parametros = catalogo
        .parametros()
        .map_err(|_| "falha ao ler os parâmetros".to_owned())?;
    let atual = Motor::novo(vigentes.clone(), parametros.clone(), exec.clone());
    let mut com_alvo: Vec<VersaoDeRegra> =
        vigentes.into_iter().filter(|v| v.regra != regra).collect();
    com_alvo.push(alvo.clone());
    let candidato = Motor::novo(com_alvo, parametros, exec);

    let item = &ctx.tela.item;
    let leitura_do_item = candidato.ler(item);
    let no_item = no_item(&leitura_do_item, alvo, item, pedido);

    // Exemplos da regra (a versão sozinha, como o editor roda).
    let relatorio = catalogo
        .testar_versao(alvo, Some(ctx.executor.as_ref()))
        .map_err(|_| "falha ao rodar os exemplos".to_owned())?;
    let falharam: Vec<String> = relatorio.falhas();
    let exemplos = DosExemplos {
        total: relatorio.resultados.len(),
        passaram: relatorio.resultados.iter().filter(|r| r.passou).count(),
        falharam: falharam.into_iter().take(5).collect(),
    };

    // Os documentos parecidos do cofre e as entradas dos exemplos.
    let parecidos = ctx.similares.similares(item, MAXIMO_DE_PARECIDOS);
    let mut amostra = DaAmostra {
        examinados: parecidos.len(),
        casam: 0,
        titulos: Vec::new(),
    };
    for e in &parecidos {
        if casou(&candidato.ler(e), regra, versao) {
            amostra.casam += 1;
            if amostra.titulos.len() < MAXIMO_DE_TITULOS {
                amostra.titulos.push(titulo_da_entrada(e));
            }
        }
    }
    let mut para_o_diff: Vec<Entrada> = Vec::with_capacity(parecidos.len() + 1);
    para_o_diff.push(item.clone());
    para_o_diff.extend(parecidos);
    let diff = simular(&atual, &candidato, &para_o_diff);

    let (mut empates, mut derrubadas, mut novos, mut perdem) =
        (Vec::<Afetado>::new(), Vec::<Afetado>::new(), 0, 0);
    let soma = |lista: &mut Vec<Afetado>, r: &str| match lista.iter_mut().find(|a| a.regra == r) {
        Some(a) => a.quantos += 1,
        None => lista.push(Afetado {
            regra: r.to_owned(),
            quantos: 1,
        }),
    };
    for m in &diff.mudaram {
        match (&m.antes, &m.depois) {
            (ResumoDaDecisao::Reconhecido { regra: x, .. }, ResumoDaDecisao::Ambiguo { .. })
                if x != regra =>
            {
                soma(&mut empates, x);
            }
            (ResumoDaDecisao::Ambiguo { regras }, _) => {
                for x in regras.iter().filter(|x| x.as_str() != regra) {
                    soma(&mut empates, x);
                }
            }
            (
                ResumoDaDecisao::Reconhecido { regra: x, .. },
                ResumoDaDecisao::Reconhecido { regra: y, .. },
            ) if x != regra && y == regra => soma(&mut derrubadas, x),
            (ResumoDaDecisao::Nenhuma, ResumoDaDecisao::Reconhecido { regra: y, .. })
                if y == regra =>
            {
                novos += 1;
            }
            (ResumoDaDecisao::Reconhecido { regra: x, .. }, ResumoDaDecisao::Nenhuma)
                if x == regra =>
            {
                perdem += 1;
            }
            _ => {}
        }
    }

    let (passou, motivo) = if !no_item.casa {
        (false, "o item não casa com o rascunho".to_owned())
    } else if let DecisaoNoItem::Ambigua(_) = no_item.decisao {
        (false, "o rascunho empata com outro padrão neste item; acrescente uma condição que separe os dois".to_owned())
    } else if let DecisaoNoItem::Outra(r) = &no_item.decisao {
        (false, format!("o padrão `{r}` vence neste item"))
    } else if !no_item.faltam.is_empty() {
        (
            false,
            format!(
                "o item casa, mas estes campos têm problema (não saíram, saíram pela metade ou o pedido pede que entrem em `detectar`): {}",
                no_item.faltam.join(", ")
            ),
        )
    } else if !exemplos.falharam.is_empty() {
        (false, "há exemplos que falham".to_owned())
    } else {
        (true, String::new())
    };
    Ok(PreTeste {
        regra: regra.to_owned(),
        versao,
        item: no_item,
        exemplos,
        amostra,
        empates,
        derrubadas,
        novos,
        perdem,
        passou,
        motivo,
    })
}
