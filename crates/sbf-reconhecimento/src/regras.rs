//! O motor de regras de reconhecimento ligado ao reconhecimento de documentos (spec 13,
//! ADR-021).
//!
//! - **Leitura**: no passo "Leitores locais", [`ler_com_regras`] monta a [`Entrada`], consulta
//!   as versões ativas e consolidadas do `regras.sqlite` do cofre e decide: o leitor de
//!   contracheque segue o caminho nativo de sempre (e o motor só completa com o script do
//!   IRRF, se a versão o tiver); outro tipo vira [`Reconhecido::PelaRegra`]; empate vira
//!   [`Reconhecido::Ambiguo`] (nada é gravado); sem regra, o caminho atual.
//! - **Sugestões**: as propostas abstratas da regra (`ProvarRegistro`, `ConciliarOcorrencia`,
//!   `PagarFatura`) são resolvidas contra as mesmas fontes que o planejamento usa para
//!   candidatos de conciliação (o extrato da Pluggy no inventário e
//!   `candidatos_de_conciliacao`). Nenhuma proposta grava sozinha: só vira [`Sugestao`], e só
//!   é `automatica` quando a regra está **consolidada** e as três provas batem exatamente (D4).
//! - **Registros da Pluggy** (D13): [`entrada_do_registro`] e [`ler_registro`]; os campos
//!   extraídos só enriquecem o rótulo dos candidatos ([`rotulo_do_candidato`]).
//!
//! Nada de conteúdo de documento vai para log ou erro.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use sbf_planejamento::conciliacao::normalizar;
use sbf_planejamento::{
    AlvoDoDocumento, Banco, Centavos, Competencia, Conciliacao, Data, EstadoDaOcorrencia, Filtro,
    FonteDeMovimentos as _, MovimentoCandidato, MovimentoDoExtrato, Natureza, Ocorrencia, Periodo,
    Planejamento, PlanejamentoSqlite, SemSaldos, TipoDeDocumento,
};
use sbf_planilha::contracheque::Contracheque;
use sbf_planilha::movimentos::MovimentosDaPluggy;
use sbf_regras::catalogo::{Catalogo, EstadoDaProposta};
use sbf_regras::motor::Motor;
use sbf_regras::script::ExecutorDeScript;
use sbf_regras::{
    Campo, Canal, DeOnde, Decisao, Dicas, Entrada, ErroDasRegras, Estado as EstadoDaRegra, Origem,
    Proposta, Quem, Reconhecimento, Retorno, TipoDeEntrada, VERSAO_DO_ESQUEMA, Valor,
};

use crate::cofre::{Estado, ReconhecedorDoCofre, ler_contracheque};
use crate::{ErroDoReconhecimento, PROVAS_POSSIVEIS, Reconhecido, Situacao, Sugestao};

/// O catálogo das regras, na pasta do cofre.
pub const ARQUIVO_DAS_REGRAS: &str = "regras.sqlite";

/// O leitor nativo do contracheque (o nome que a regra de fábrica declara).
pub const LEITOR_DO_CONTRACHEQUE: &str = "contracheque";

/// O que `com_regras` guarda: o executor dos scripts, se houver.
#[derive(Debug, Clone)]
pub struct Ligadas {
    pub executor: Option<Arc<dyn ExecutorDeScript>>,
}

// ---------------------------------------------------------------------------------------
// Entradas
// ---------------------------------------------------------------------------------------

/// O tipo do conteúdo pela assinatura do arquivo (nunca pela extensão): `%PDF`, PNG, JPEG;
/// texto quando os primeiros bytes são UTF-8 sem NUL; senão `application/octet-stream`.
pub fn mime_pelo_conteudo(inicio: &[u8]) -> &'static str {
    if inicio.starts_with(b"%PDF") {
        "application/pdf"
    } else if inicio.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if inicio.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if !inicio.is_empty() && !inicio.contains(&0) && utf8_aparado(inicio) {
        "text/plain"
    } else {
        "application/octet-stream"
    }
}

/// Os bytes são UTF-8, tolerando um caractere cortado no fim da amostra.
fn utf8_aparado(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none(),
    }
}

/// A entrada de um documento (arquivo): o id é o sha256.
pub fn entrada_do_documento(
    sha256: &str,
    nome: &str,
    mime: &str,
    texto: Option<&str>,
    canal: Canal,
    id_externo: Option<&str>,
) -> Entrada {
    Entrada {
        versao_do_esquema: VERSAO_DO_ESQUEMA,
        id: sha256.to_owned(),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal,
            id_externo: id_externo.map(str::to_owned),
        },
        sha256: Some(sha256.to_owned()),
        mime: Some(mime.to_owned()),
        nome: Some(nome.to_owned()),
        texto: texto.map(str::to_owned),
        dicas: Dicas::default(),
        momento: None,
    }
}

/// A entrada de um registro da Pluggy (D13): id `pluggy:<id>`, tipo Registro, e as dicas
/// que o banco já traz (descrição, valor, data e conta).
pub fn entrada_do_registro(m: &MovimentoDoExtrato) -> Entrada {
    let id = if m.movimento.starts_with("pluggy:") {
        m.movimento.clone()
    } else {
        format!("pluggy:{}", m.movimento)
    };
    Entrada {
        versao_do_esquema: VERSAO_DO_ESQUEMA,
        id: id.clone(),
        tipo: TipoDeEntrada::Registro,
        origem: Origem {
            canal: Canal::Pluggy,
            id_externo: Some(id),
        },
        sha256: None,
        mime: None,
        nome: None,
        texto: None,
        dicas: Dicas {
            conta: (!m.conta_do_extrato.trim().is_empty()).then(|| m.conta_do_extrato.clone()),
            valor_centavos: Some(m.valor.0),
            data: Some(m.data.to_string()),
            descricao: (!m.descricao.trim().is_empty()).then(|| m.descricao.clone()),
            contraparte: m.contraparte.clone().filter(|c| !c.trim().is_empty()),
            contraparte_doc: m.contraparte_doc.clone().filter(|c| !c.trim().is_empty()),
        },
        momento: Some(m.data.to_string()),
    }
}

/// Lê um registro da Pluggy com o motor: o reconhecimento da regra vencedora, se houver.
/// Ambíguo ou sem regra é `None`.
pub fn ler_registro(motor: &Motor, entrada: &Entrada) -> Option<Reconhecimento> {
    match motor.ler(entrada).decisao {
        Decisao::Reconhecido(r) => Some(*r),
        Decisao::Ambiguo { .. } | Decisao::Nenhuma => None,
    }
}

/// O rótulo de um candidato de conciliação, enriquecido com o que as regras extraíram da
/// descrição (favorecido, tipo de transferência). Só o rótulo: as assinaturas e o casamento
/// do planejamento seguem os mesmos (D13).
pub fn rotulo_do_candidato(motor: &Motor, candidato: &MovimentoCandidato) -> String {
    let base = candidato.movimento.descricao.clone();
    let Some(rec) = ler_registro(motor, &entrada_do_registro(&candidato.movimento)) else {
        return base;
    };
    let texto = |nome: &str| match rec.campos.get(nome).map(|c| &c.valor) {
        Some(Valor::Texto(t)) if !t.trim().is_empty() => Some(t.trim().to_owned()),
        _ => None,
    };
    let mut rotulo = base;
    if let Some(f) = texto("favorecido") {
        rotulo = format!("{rotulo} · favorecido {f}");
    }
    if let Some(t) = texto("tipo_de_transferencia") {
        rotulo = format!("{rotulo} · {t}");
    }
    rotulo
}

// ---------------------------------------------------------------------------------------
// Retorno do usuário
// ---------------------------------------------------------------------------------------

/// O que o usuário fez com a proposta de `chave`: aceitou, corrigiu, rejeitou ou desfez.
/// Alimenta a consolidação (D4); a janela chama.
///
/// # Errors
/// [`ErroDasRegras`] (catálogo que não abre, chave desconhecida, retorno fora de ordem).
pub fn registrar_retorno(cofre: &Path, chave: &str, retorno: Retorno) -> Result<(), ErroDasRegras> {
    let mut catalogo = Catalogo::abrir(&cofre.join(ARQUIVO_DAS_REGRAS))?;
    catalogo.registrar_retorno(chave, retorno, &Quem::Usuario, "")
}

// ---------------------------------------------------------------------------------------
// Contracheque: campos e IRRF
// ---------------------------------------------------------------------------------------

fn do_leitor(valor: Valor) -> Campo {
    Campo {
        valor,
        de_onde: DeOnde::Leitor {
            leitor: LEITOR_DO_CONTRACHEQUE.to_owned(),
        },
    }
}

/// O IRRF retido no mês: a soma dos descontos de imposto de renda da competência, **sem** o
/// do PLR e sem as rubricas informativas.
pub fn irrf_retido(c: &Contracheque) -> i64 {
    c.rubricas
        .iter()
        .zip(&c.informativas)
        .filter(|(r, informativa)| {
            let nome = normalizar(&r.nome);
            let de_ir = nome.contains("imposto de renda")
                || nome.contains("irrf")
                || r.codigo.as_deref() == Some("4325");
            !**informativa
                && r.tipo == sbf_planejamento::TipoDeRubrica::Desconto
                && de_ir
                && !nome.contains("plr")
                && r.competencia.is_none_or(|k| k == c.pagamento)
        })
        .map(|(r, _)| r.valor.0)
        .sum()
}

/// Os campos que o leitor de contracheque entrega ao motor (`Motor::completar`). O
/// rendimento tributável só entraria se o contracheque o trouxesse de forma inequívoca; o
/// layout atual não traz (a base de cálculo e o "Salário Contrib. INSS" são outras coisas).
pub fn campos_do_contracheque(c: &Contracheque) -> BTreeMap<String, Campo> {
    let mut campos = BTreeMap::new();
    let liquido = c.liquido.unwrap_or_else(|| c.liquido_do_mes());
    campos.insert(
        "liquido_centavos".to_owned(),
        do_leitor(Valor::Centavos(liquido.0)),
    );
    campos.insert(
        "competencia".to_owned(),
        do_leitor(Valor::Competencia(format!(
            "{:04}-{:02}",
            c.pagamento.ano, c.pagamento.mes
        ))),
    );
    if let Some(base) = c.base_irrf {
        campos.insert(
            "base_irrf_centavos".to_owned(),
            do_leitor(Valor::Centavos(base.0)),
        );
    }
    campos.insert(
        "irrf_retido_centavos".to_owned(),
        do_leitor(Valor::Centavos(irrf_retido(c))),
    );
    campos
}

/// `R$ 1.234,56`.
fn reais(centavos: i64) -> String {
    let sinal = if centavos < 0 { "-" } else { "" };
    let n = centavos.unsigned_abs();
    let (inteiro, frac) = (n / 100, n % 100);
    let digitos = inteiro.to_string();
    let mut agrupado = String::new();
    for (i, d) in digitos.chars().enumerate() {
        if i > 0 && (digitos.len() - i) % 3 == 0 {
            agrupado.push('.');
        }
        agrupado.push(d);
    }
    format!("{sinal}R$ {agrupado},{frac:02}")
}

fn centavos_do_campo(rec: &Reconhecimento, nome: &str) -> Option<i64> {
    match rec.campos.get(nome).map(|c| &c.valor) {
        Some(Valor::Centavos(n)) => Some(*n),
        _ => None,
    }
}

/// O aviso do IRRF (AC-LUA-11): o estimado pela tabela difere do retido. Só alerta; a rubrica
/// do contracheque nunca muda.
pub fn aviso_do_irrf(rec: &Reconhecimento) -> Option<String> {
    let estimado = centavos_do_campo(rec, "irrf_estimado_centavos")?;
    let diferenca = centavos_do_campo(rec, "irrf_diferenca_centavos")?;
    if !rec.revisar || diferenca == 0 {
        return None;
    }
    let tabela = rec
        .evidencias
        .iter()
        .filter_map(|e| e.strip_prefix("tabela:irrf:"))
        .find(|v| v.chars().any(|c| !c.is_ascii_digit()))
        .unwrap_or("tabela do IRRF");
    Some(format!(
        "⚠ IRRF estimado {} difere do retido {} ({tabela})",
        reais(estimado),
        reais(estimado + diferenca)
    ))
}

// ---------------------------------------------------------------------------------------
// O passo "Leitores locais" com o motor
// ---------------------------------------------------------------------------------------

/// Roda o motor no passo de leitura local. `None` quando as regras não estão ligadas, o
/// catálogo não abre ou nenhuma regra se aplica: aí vale o caminho de sempre (AC-LUA-14).
pub(crate) fn ler_com_regras(
    reconhecedor: &ReconhecedorDoCofre,
    estado: &mut Estado,
    caminho: &Path,
) -> Option<Situacao> {
    let ligadas = reconhecedor.regras.as_ref()?;
    let mut catalogo = Catalogo::abrir(&reconhecedor.cofre.join(ARQUIVO_DAS_REGRAS)).ok()?;
    catalogo.semear_fabrica().ok()?;
    let motor = Motor::das_vigentes(&catalogo, ligadas.executor.clone()).ok()?;

    let (canal, id_externo) = match &estado.origem {
        Some((canal, id)) => (*canal, Some(id.as_str())),
        None if estado.dentro_do_cofre => (Canal::Cofre, None),
        None => (Canal::Arrastado, None),
    };
    let nome = caminho
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let entrada = entrada_do_documento(
        &estado.sha256,
        &nome,
        mime_do_arquivo(caminho),
        estado.conteudo.as_deref(),
        canal,
        id_externo,
    );
    let mut leitura = motor.ler(&entrada);

    let mut situacao = None;
    match leitura.decisao.clone() {
        Decisao::Reconhecido(rec)
            if rec.leitor.as_deref() == Some(LEITOR_DO_CONTRACHEQUE)
                && estado.conteudo.is_some() =>
        {
            // O leitor nativo de sempre; o motor só completa com o que a versão tiver
            // (o script do IRRF).
            let s = ler_contracheque(estado);
            if let Reconhecido::Contracheque {
                contracheque: c, ..
            } = &estado.reconhecido
            {
                let pronto = motor.completar(*rec, campos_do_contracheque(c), &entrada);
                if let Some(aviso) = aviso_do_irrf(&pronto) {
                    estado.avisos.push(aviso);
                }
                leitura.decisao = Decisao::Reconhecido(Box::new(pronto.clone()));
                estado.regra = Some(pronto);
            }
            situacao = Some(s);
        }
        Decisao::Reconhecido(rec) if rec.leitor.is_none() => {
            estado
                .avisos
                .extend(rec.motivos.iter().filter(|m| m.starts_with('⚠')).cloned());
            estado.regra = Some((*rec).clone());
            estado.reconhecido = Reconhecido::PelaRegra {
                reconhecimento: rec,
            };
            situacao = Some(Situacao::Feita);
        }
        Decisao::Ambiguo { candidatas, motivo } => {
            estado.reconhecido = Reconhecido::Ambiguo { candidatas, motivo };
            situacao = Some(Situacao::Feita);
        }
        // Sem regra (ou um leitor que o sbf não conhece): o caminho de sempre.
        Decisao::Reconhecido(_) | Decisao::Nenhuma => {}
    }

    let gravou = catalogo.registrar_execucao(&leitura).is_ok();
    let propostas_ok = match &estado.regra {
        Some(rec) if !rec.propostas.is_empty() => {
            catalogo.registrar_proposta(rec, &entrada.id).is_ok()
        }
        _ => true,
    };
    if !(gravou && propostas_ok) {
        estado
            .avisos
            .push("não consegui gravar o rastro das regras de reconhecimento".to_owned());
    }
    estado.entrada = Some(entrada);
    estado.leitura = Some(leitura);
    situacao
}

/// Uma mensagem sem arquivo (o corpo de um e-mail, um texto do Telegram) que chegou por um
/// canal e vai passar pelas regras.
#[derive(Debug, Clone, Copy)]
pub struct MensagemRecebida<'a> {
    pub canal: Canal,
    /// A identidade estável da entrada (`gmail:<id da mensagem>`).
    pub id: &'a str,
    /// O id no canal (o id da mensagem), que a regra vê em `Entrada.origem`.
    pub id_externo: &'a str,
    /// O assunto e o corpo, como vieram.
    pub texto: &'a str,
    /// Quando chegou, ISO 8601, se o canal diz.
    pub momento: Option<&'a str>,
    /// `AAAA-MM-DD`, se o canal diz.
    pub data: Option<&'a str>,
}

impl ReconhecedorDoCofre {
    /// Passa uma mensagem pelas regras de reconhecimento (entrada tipo `Mensagem`). Grava só o
    /// rastro das regras (execução e propostas abertas); nada vai para o planejamento e a
    /// mensagem em si não é gravada aqui (quem a entrega guarda no inventário, se quiser).
    ///
    /// Sem `com_regras`, ou com o catálogo fechado, devolve [`Reconhecido::Nada`].
    ///
    /// # Errors
    /// Nenhum hoje: falhas do catálogo viram [`Reconhecido::Nada`] com o motivo genérico.
    pub fn reconhecer_mensagem(
        &self,
        m: &MensagemRecebida<'_>,
    ) -> Result<Reconhecido, ErroDoReconhecimento> {
        let nada = |motivo: &str| {
            Ok(Reconhecido::Nada {
                motivo: motivo.to_owned(),
            })
        };
        let Some(ligadas) = self.regras.as_ref() else {
            return nada("as regras de reconhecimento não estão ligadas");
        };
        let Ok(mut catalogo) = Catalogo::abrir(&self.cofre.join(ARQUIVO_DAS_REGRAS)) else {
            return nada("não deu para abrir o catálogo de regras");
        };
        if catalogo.semear_fabrica().is_err() {
            return nada("não deu para preparar o catálogo de regras");
        }
        let Ok(motor) = Motor::das_vigentes(&catalogo, ligadas.executor.clone()) else {
            return nada("não deu para montar o motor de regras");
        };
        let entrada = Entrada {
            versao_do_esquema: VERSAO_DO_ESQUEMA,
            id: m.id.to_owned(),
            tipo: TipoDeEntrada::Mensagem,
            origem: Origem {
                canal: m.canal,
                id_externo: Some(m.id_externo.to_owned()),
            },
            sha256: None,
            mime: None,
            nome: None,
            texto: Some(m.texto.to_owned()),
            dicas: Dicas {
                data: m.data.map(str::to_owned),
                ..Dicas::default()
            },
            momento: m.momento.map(str::to_owned),
        };
        let leitura = motor.ler(&entrada);
        let reconhecido = match leitura.decisao.clone() {
            Decisao::Reconhecido(rec) => Reconhecido::PelaRegra {
                reconhecimento: rec,
            },
            Decisao::Ambiguo { candidatas, motivo } => Reconhecido::Ambiguo { candidatas, motivo },
            Decisao::Nenhuma => Reconhecido::Nada {
                motivo: "nenhuma regra reconheceu a mensagem".to_owned(),
            },
        };
        let gravou = catalogo.registrar_execucao(&leitura).is_ok();
        let propostas_ok = match &reconhecido {
            Reconhecido::PelaRegra { reconhecimento } if !reconhecimento.propostas.is_empty() => {
                catalogo
                    .registrar_proposta(reconhecimento, &entrada.id)
                    .is_ok()
            }
            _ => true,
        };
        if !(gravou && propostas_ok) {
            return nada("não consegui gravar o rastro das regras de reconhecimento");
        }
        Ok(reconhecido)
    }
}

fn mime_do_arquivo(caminho: &Path) -> &'static str {
    use std::io::Read as _;
    let mut inicio = [0u8; 512];
    let n = std::fs::File::open(caminho)
        .and_then(|mut f| f.read(&mut inicio))
        .unwrap_or(0);
    mime_pelo_conteudo(&inicio[..n])
}

// ---------------------------------------------------------------------------------------
// Propostas → sugestões
// ---------------------------------------------------------------------------------------

struct Contexto<'a> {
    fonte: MovimentosDaPluggy,
    plan: Option<&'a PlanejamentoSqlite>,
    rec: &'a Reconhecimento,
    sha: &'a str,
    hoje: Data,
    consolidada: bool,
}

/// Resolve as propostas da regra contra o extrato e o planejamento do cofre. Não escreve
/// nada. Uma proposta já decidida (aceita, recusada, desfeita) não volta.
///
/// # Errors
/// O motivo, genérico, quando o catálogo ou o planejamento não leem.
pub(crate) fn sugerir_das_propostas(
    cofre: &Path,
    rec: &Reconhecimento,
    sha: &str,
    _entrada: Option<&Entrada>,
    avisos: &mut Vec<String>,
) -> Result<Vec<Sugestao>, String> {
    let catalogo = Catalogo::abrir(&cofre.join(ARQUIVO_DAS_REGRAS))
        .map_err(|_| "não deu para abrir o catálogo de regras".to_owned())?;
    let abertas = catalogo
        .propostas(Some(EstadoDaProposta::Aberta))
        .map_err(|_| "não deu para ler as propostas das regras".to_owned())?;
    if !abertas.iter().any(|p| p.chave == rec.chave) {
        return Ok(Vec::new());
    }
    let caminho = cofre.join("planejamento.sqlite");
    let plan = if caminho.is_file() {
        let banco =
            Banco::abrir(&caminho).map_err(|_| "não deu para abrir o planejamento".to_owned())?;
        Some(
            PlanejamentoSqlite::novo(banco, Box::new(SemSaldos))
                .com_movimentos(Box::new(MovimentosDaPluggy::do_cofre(cofre))),
        )
    } else {
        None
    };
    let ctx = Contexto {
        fonte: MovimentosDaPluggy::do_cofre(cofre),
        plan: plan.as_ref(),
        rec,
        sha,
        hoje: Data::hoje_em_sao_paulo(),
        consolidada: rec.estado == EstadoDaRegra::Consolidada,
    };
    let mut sugestoes = Vec::new();
    for proposta in &rec.propostas {
        let resolvida = match proposta {
            Proposta::ProvarRegistro {
                valor_centavos,
                data,
                janela_dias,
                conta,
            } => provar_registro(&ctx, *valor_centavos, data, *janela_dias, conta.as_deref()),
            Proposta::ConciliarOcorrencia {
                valor_centavos,
                data,
                janela_dias,
                compromisso,
            } => conciliar_ocorrencia(
                &ctx,
                *valor_centavos,
                data,
                *janela_dias,
                compromisso.as_deref(),
            ),
            Proposta::PagarFatura {
                valor_centavos,
                vencimento,
                cartao,
            } => pagar_fatura(&ctx, *valor_centavos, vencimento, cartao.as_deref()),
        }
        .map_err(|_| "não deu para ler o planejamento".to_owned())?;
        match resolvida {
            Resolvida::Sugestao(s) => sugestoes.push(*s),
            Resolvida::Nada(motivo) => avisos.push(motivo),
        }
    }
    Ok(sugestoes)
}

enum Resolvida {
    Sugestao(Box<Sugestao>),
    /// Um aviso para o cartão: a proposta não achou onde se apoiar.
    Nada(String),
}

type Achado<T> = Result<T, sbf_planejamento::ErroDoPlanejamento>;

fn mesma_conta(a: &str, b: &str) -> bool {
    normalizar(a).trim() == normalizar(b).trim()
}

fn dia_mes(d: Data) -> String {
    format!("{:02}/{:02}", d.dia, d.mes)
}

/// Escolhe o único candidato; com mais de um, o único cuja conta confere; senão nenhum
/// (empate não se adivinha).
fn escolher_unico(
    mut candidatos: Vec<(MovimentoDoExtrato, bool)>,
) -> Option<(MovimentoDoExtrato, bool)> {
    if candidatos.len() > 1 {
        candidatos.retain(|(_, conta)| *conta);
    }
    match candidatos.len() {
        1 => candidatos.pop(),
        _ => None,
    }
}

fn livre(o: &Ocorrencia) -> bool {
    o.conciliado.is_none()
        && !matches!(o.natureza, Natureza::DescontoEmFolha)
        && matches!(
            o.estado,
            EstadoDaOcorrencia::Prevista
                | EstadoDaOcorrencia::PagoInformado
                | EstadoDaOcorrencia::Parcial
                | EstadoDaOcorrencia::EmAtraso
                | EstadoDaOcorrencia::Adiada
        )
}

/// As ocorrências do planejamento que vencem até `dias` de `centro`.
fn ocorrencias_perto(ctx: &Contexto<'_>, centro: Data, dias: i64) -> Achado<Vec<Ocorrencia>> {
    let Some(plan) = ctx.plan else {
        return Ok(Vec::new());
    };
    let competencias: BTreeSet<Competencia> =
        [centro.somar_dias(-dias), centro, centro.somar_dias(dias)]
            .into_iter()
            .map(Competencia::de_data)
            .collect();
    let mut saida: Vec<Ocorrencia> = Vec::new();
    for c in competencias {
        let controle = plan.controle(&Periodo::MesCivil(c), Filtro::Todos, None, ctx.hoje)?;
        for o in controle
            .linhas
            .into_iter()
            .chain(controle.atrasadas_de_antes)
        {
            if !saida.iter().any(|x| x.id == o.id) {
                saida.push(o);
            }
        }
    }
    Ok(saida
        .into_iter()
        .filter(|o| o.vencimento.dias_ate(centro).abs() <= dias)
        .collect())
}

/// Os movimentos que as conciliações confirmadas das ocorrências perto de `centro` já seguram.
fn movimentos_ja_conciliados(
    ctx: &Contexto<'_>,
    centro: Data,
    dias: i64,
) -> Achado<BTreeSet<String>> {
    let Some(plan) = ctx.plan else {
        return Ok(BTreeSet::new());
    };
    let mut usados = BTreeSet::new();
    for o in ocorrencias_perto(ctx, centro, dias)?
        .iter()
        .filter(|o| o.conciliado.is_some())
    {
        for g in plan.conciliacoes_da_ocorrencia(&o.id)? {
            if g.conciliacao.confirmada {
                usados.insert(g.conciliacao.movimento);
            }
        }
    }
    Ok(usados)
}

fn regra_da(ctx: &Contexto<'_>) -> (String, u32) {
    (ctx.rec.regra.clone(), ctx.rec.versao)
}

/// A regra só concilia sozinha quando está consolidada e as três provas batem.
fn automatica(ctx: &Contexto<'_>, provas: u8, desfeita: bool) -> bool {
    ctx.consolidada && provas >= PROVAS_POSSIVEIS && !desfeita
}

fn provar_registro(
    ctx: &Contexto<'_>,
    valor: i64,
    data: &str,
    janela: u32,
    conta: Option<&str>,
) -> Achado<Resolvida> {
    let Some(d) = Data::de_iso(data) else {
        return Ok(Resolvida::Nada(
            "o comprovante traz uma data que não leio; não achei o débito".to_owned(),
        ));
    };
    let dias = i64::from(janela);
    let mut candidatos: Vec<MovimentoDoExtrato> = ctx
        .fonte
        .movimentos(d.somar_dias(-dias), d.somar_dias(dias))
        .into_iter()
        .filter(|m| m.valor.0.abs() == valor.abs())
        .collect();
    // Um comprovante prova um débito; só se não há nenhum aceita o crédito de mesmo valor.
    if candidatos.iter().any(|m| m.valor.0 < 0) {
        candidatos.retain(|m| m.valor.0 < 0);
    }
    let com_conta: Vec<(MovimentoDoExtrato, bool)> = candidatos
        .into_iter()
        .map(|m| {
            let ok = conta.is_some_and(|c| mesma_conta(c, &m.conta_do_extrato));
            (m, ok)
        })
        .collect();
    let Some((movimento, conta_ok)) = escolher_unico(com_conta) else {
        return Ok(Resolvida::Nada(
            "não achei um único débito no extrato que o comprovante prove".to_owned(),
        ));
    };
    // O débito já conciliado a uma ocorrência: é nela que o comprovante se anexa.
    let mut alvo = None;
    if let Some(plan) = ctx.plan {
        for o in ocorrencias_perto(ctx, movimento.data, 20)?
            .iter()
            .filter(|o| o.conciliado.is_some())
        {
            let tem = plan.conciliacoes_da_ocorrencia(&o.id)?.iter().any(|g| {
                g.conciliacao.confirmada && g.conciliacao.movimento == movimento.movimento
            });
            if tem {
                alvo = Some(AlvoDoDocumento::Ocorrencia { id: o.id.clone() });
                break;
            }
        }
        if let Some(a) = &alvo
            && plan
                .documentos(a)?
                .iter()
                .any(|d| d.sha256 == ctx.sha && !d.desvinculado)
        {
            return Ok(Resolvida::Nada(
                "o comprovante já está ligado a este débito".to_owned(),
            ));
        }
    }
    let provas = 1 + 1 + u8::from(conta_ok);
    Ok(Resolvida::Sugestao(Box::new(Sugestao::ProvarRegistro {
        rotulo: format!(
            "Este comprovante prova o débito de {} · {}",
            dia_mes(movimento.data),
            reais(movimento.valor.0.abs())
        ),
        movimento,
        automatica: automatica(ctx, provas, false) && alvo.is_some(),
        alvo,
        tipo: TipoDeDocumento::Comprovante,
        regra: regra_da(ctx),
        chave: ctx.rec.chave.clone(),
        provas,
    })))
}

fn conciliar_ocorrencia(
    ctx: &Contexto<'_>,
    valor: i64,
    data: &str,
    janela: u32,
    compromisso: Option<&str>,
) -> Achado<Resolvida> {
    let (Some(plan), Some(d)) = (ctx.plan, Data::de_iso(data)) else {
        return Ok(Resolvida::Nada(
            "sem planejamento (ou data ilegível) para conciliar a ocorrência".to_owned(),
        ));
    };
    let nome = compromisso.map(|c| normalizar(c).trim().to_owned());
    let candidatas: Vec<Ocorrencia> = ocorrencias_perto(ctx, d, i64::from(janela))?
        .into_iter()
        .filter(livre)
        .filter(|o| match &nome {
            Some(n) if !n.is_empty() => normalizar(&o.nome).contains(n.as_str()),
            _ => o.previsto.is_some_and(|p| p.0.abs() == valor.abs()),
        })
        .collect();
    let [ocorrencia] = candidatas.as_slice() else {
        return Ok(Resolvida::Nada(
            "não achei uma única ocorrência do compromisso para conciliar".to_owned(),
        ));
    };
    let documento = u8::from(nome.as_deref().is_some_and(|n| !n.is_empty()));
    let candidatos: Vec<MovimentoCandidato> = plan
        .candidatos_de_conciliacao(&ocorrencia.id)?
        .into_iter()
        .filter(|c| c.movimento.valor.0.abs() == valor.abs())
        .collect();
    let escolhido = escolher_unico(
        candidatos
            .iter()
            .map(|c| (c.movimento.clone(), c.mesma_conta || c.bate_assinatura))
            .collect(),
    );
    let Some((movimento, conta_ok)) = escolhido else {
        return Ok(Resolvida::Nada(
            "não achei no extrato um único débito de valor exato para esta ocorrência".to_owned(),
        ));
    };
    let desfeita = plan.conciliacao_desfeita(&ocorrencia.id, &movimento.movimento)?;
    let provas = documento + 1 + u8::from(conta_ok);
    Ok(Resolvida::Sugestao(Box::new(
        Sugestao::ConciliarOcorrencia {
            rotulo: format!(
                "Conciliar {} · {:02}/{} com o débito de {} no extrato",
                ocorrencia.nome,
                ocorrencia.id.competencia.mes,
                ocorrencia.id.competencia.ano,
                dia_mes(movimento.data)
            ),
            conciliacao: Conciliacao {
                ocorrencia: ocorrencia.id.clone(),
                movimento: movimento.movimento.clone(),
                valor: Centavos(movimento.valor.0.abs()),
                data: movimento.data,
                confirmada: false,
            },
            regra: regra_da(ctx),
            chave: ctx.rec.chave.clone(),
            automatica: automatica(ctx, provas, desfeita),
            provas,
        },
    )))
}

/// Tipos de movimento que nunca pagam fatura (o `pagamento_fatura` é justamente o que serve).
const TIPOS_QUE_NAO_PAGAM_FATURA: [&str; 3] = ["transferencia_interna", "investimento", "estorno"];

fn pagar_fatura(
    ctx: &Contexto<'_>,
    valor: i64,
    vencimento: &str,
    cartao: Option<&str>,
) -> Achado<Resolvida> {
    let (Some(plan), Some(v)) = (ctx.plan, Data::de_iso(vencimento)) else {
        return Ok(Resolvida::Nada(
            "sem planejamento (ou vencimento ilegível) para achar a ocorrência da fatura"
                .to_owned(),
        ));
    };
    let cartao = cartao
        .map(|c| normalizar(c).trim().to_owned())
        .filter(|c| !c.is_empty());
    let candidatas: Vec<Ocorrencia> = ocorrencias_perto(ctx, v, 7)?
        .into_iter()
        .filter(livre)
        .filter(|o| matches!(o.natureza, Natureza::Saida))
        .filter(|o| {
            let nome = normalizar(&o.nome);
            match &cartao {
                Some(c) => nome.contains(c.as_str()),
                None => nome.contains("fatura"),
            }
        })
        .collect();
    let [ocorrencia] = candidatas.as_slice() else {
        return Ok(Resolvida::Nada(
            "não achei uma única ocorrência da fatura para pagar".to_owned(),
        ));
    };
    let usados = movimentos_ja_conciliados(ctx, v, 15)?;
    let apelido = ocorrencia
        .conta
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    let conta_do_extrato = plan
        .contas()?
        .into_iter()
        .find(|c| c.apelido.trim() == apelido)
        .and_then(|c| c.conta_do_extrato);
    let assinatura = plan.assinatura(&ocorrencia.id.recorrente)?;
    let candidatos: Vec<(MovimentoDoExtrato, bool)> = ctx
        .fonte
        .movimentos(v.somar_dias(-10), v.somar_dias(10))
        .into_iter()
        .filter(|m| {
            m.valor.0 == -valor.abs()
                && !usados.contains(&m.movimento)
                && !TIPOS_QUE_NAO_PAGAM_FATURA.contains(&m.tipo.as_str())
        })
        .map(|m| {
            let na_conta = conta_do_extrato
                .as_deref()
                .is_some_and(|c| mesma_conta(c, &m.conta_do_extrato));
            let na_assinatura = assinatura.as_ref().is_some_and(|a| {
                let t = normalizar(&a.texto);
                !t.trim().is_empty() && normalizar(&m.descricao).contains(t.trim())
            });
            (m, na_conta || na_assinatura)
        })
        .collect();
    let Some((movimento, conta_ok)) = escolher_unico(candidatos) else {
        return Ok(Resolvida::Nada(
            "não achei no extrato um único débito de valor exato para pagar a fatura".to_owned(),
        ));
    };
    let desfeita = plan.conciliacao_desfeita(&ocorrencia.id, &movimento.movimento)?;
    let documento = u8::from(cartao.is_some());
    let provas = documento + 1 + u8::from(conta_ok);
    Ok(Resolvida::Sugestao(Box::new(Sugestao::PagarFatura {
        rotulo: format!(
            "Pagar a fatura {} · {:02}/{} com o débito de {} (pagamento de fatura, não é gasto novo)",
            ocorrencia.nome,
            ocorrencia.id.competencia.mes,
            ocorrencia.id.competencia.ano,
            dia_mes(movimento.data)
        ),
        conciliacao: Conciliacao {
            ocorrencia: ocorrencia.id.clone(),
            movimento: movimento.movimento.clone(),
            valor: Centavos(movimento.valor.0.abs()),
            data: movimento.data,
            confirmada: false,
        },
        regra: regra_da(ctx),
        chave: ctx.rec.chave.clone(),
        automatica: automatica(ctx, provas, desfeita),
        provas,
    })))
}
