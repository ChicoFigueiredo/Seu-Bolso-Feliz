//! **Padrões visíveis** (spec 15, Frente B, D5 e D6): a lógica pura, sem egui, de três coisas
//! que deixam o usuário enxergar os padrões de reconhecimento quando eles ficam numerosos.
//!
//! - os **filtros** da lista da área Padrões ([`FiltroDePadroes`], [`filtrar`]): texto, estado,
//!   tipo de entrada, tipo real (MIME) e "casam com o documento selecionado";
//! - o **mapa** em árvore ([`montar_mapa`]): tipo de entrada, tipo documental e os padrões em
//!   ordem de prioridade e especificidade;
//! - o **"Por que este padrão?"** ([`montar_por_que`]): do que o motor já devolveu (a
//!   [`Leitura`]) para o cartão do documento. Nada é decidido aqui: o vencedor, o motivo e o
//!   "Ambíguo" vêm do motor (`Leitura.execucoes`, `motivos` do vencedor, `Decisao::Ambiguo`); só
//!   se juntam a prioridade e a especificidade de cada versão, que o motor usa e a leitura não
//!   carrega.
//!
//! O desenho está em [`crate::regras_ui`] (lista, filtros e mapa) e em
//! [`crate::reconhecimento_regras_ui`] (o cartão).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use sbf_regras::catalogo::VersaoDeRegra;
use sbf_regras::definicao::{Condicao, Definicao, normalizar};
use sbf_regras::motor::campo_em_que_discordam;
use sbf_regras::{
    Decisao, Entrada, Estado, IdDaRegra, Leitura, ResultadoDaExecucao, TipoDeEntrada,
};
use serde::{Deserialize, Serialize};

/// Quem sabe dizer, de um documento (pelo `Entrada::id`, o sha256 do arquivo), quais padrões
/// **já casaram** com ele alguma vez (a tabela `execucao`). É o ponto de extensão do filtro
/// "já casaram": a Frente A (marcação) entrega a consulta e o integrador a liga com
/// [`crate::regras::FonteDasRegras::com_ja_casaram`]. Sem ela, o modo fica indisponível.
pub type PadroesQueJaCasaram =
    Arc<dyn Fn(&str) -> Result<BTreeSet<IdDaRegra>, String> + Send + Sync>;

// ---------------------------------------------------------------------------------------
// Rótulos
// ---------------------------------------------------------------------------------------

/// O tipo de entrada como a tela o diz.
pub fn rotulo_do_tipo_de_entrada(t: TipoDeEntrada) -> &'static str {
    match t {
        TipoDeEntrada::Documento => "documento",
        TipoDeEntrada::Registro => "registro",
        TipoDeEntrada::Mensagem => "mensagem",
    }
}

/// Os tipos de entrada na ordem em que a tela os lista.
pub const TIPOS_DE_ENTRADA: [TipoDeEntrada; 3] = [
    TipoDeEntrada::Documento,
    TipoDeEntrada::Registro,
    TipoDeEntrada::Mensagem,
];

/// O estado de um padrão como a tela nova o diz ("padrão" é masculino).
pub fn rotulo_do_estado_do_padrao(e: Estado) -> &'static str {
    match e {
        Estado::Rascunho => "Rascunho",
        Estado::Ativa => "Ativo",
        Estado::Consolidada => "Consolidado",
        Estado::Desativada => "Desativado",
    }
}

fn rotulo_do_estado(e: Estado) -> &'static str {
    crate::regras::rotulo_do_estado(e)
}

fn vigente(e: Estado) -> bool {
    matches!(e, Estado::Ativa | Estado::Consolidada)
}

// ---------------------------------------------------------------------------------------
// A linha de um padrão
// ---------------------------------------------------------------------------------------

/// Um padrão como os filtros e o mapa o enxergam: a versão que o representa (a em vigor; sem
/// ela, a última) com o que dela interessa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadraoDaLista {
    pub id: IdDaRegra,
    pub nome: String,
    pub tipo_documental: String,
    pub versao: u32,
    pub estado: Estado,
    /// Há um rascunho mais novo que a versão que representa o padrão (o usuário o está
    /// editando): o filtro "rascunho" também o acha.
    pub tem_rascunho_aberto: bool,
    pub prioridade: i32,
    pub especificidade: usize,
    pub aplica_a: Vec<TipoDeEntrada>,
    /// Os tipos reais (MIME) que a detecção exige (`Condicao::Mime`), em todos os blocos.
    pub mimes: Vec<String>,
    /// As condições da detecção em texto simples, para a busca.
    pub condicoes: Vec<String>,
}

impl PadraoDaLista {
    /// A versão que representa o padrão: a em vigor (ativa ou consolidada) mais nova; sem ela,
    /// a última.
    pub fn representante(versoes: &[VersaoDeRegra]) -> Option<&VersaoDeRegra> {
        versoes
            .iter()
            .filter(|v| vigente(v.estado))
            .max_by_key(|v| v.versao)
            .or_else(|| versoes.iter().max_by_key(|v| v.versao))
    }

    /// Monta a linha de um padrão a partir de suas versões (da primeira à última). `None` sem
    /// versões.
    pub fn montar(id: &str, nome: &str, versoes: &[VersaoDeRegra]) -> Option<Self> {
        let representante = Self::representante(versoes)?;
        let tem_rascunho_aberto = representante.estado != Estado::Rascunho
            && versoes
                .iter()
                .any(|v| v.estado == Estado::Rascunho && v.versao > representante.versao);
        let d = &representante.definicao;
        Some(Self {
            id: id.to_owned(),
            nome: nome.to_owned(),
            tipo_documental: d.tipo_documental.clone(),
            versao: representante.versao,
            estado: representante.estado,
            tem_rascunho_aberto,
            prioridade: d.prioridade,
            especificidade: d.especificidade(),
            aplica_a: d.aplica_a.clone(),
            mimes: mimes_da(d),
            condicoes: condicoes_em_texto(d),
        })
    }

    /// O padrão aparece quando se filtra por este estado? O do representante, e "rascunho"
    /// também quando há um rascunho mais novo.
    pub fn tem_o_estado(&self, e: Estado) -> bool {
        self.estado == e || (e == Estado::Rascunho && self.tem_rascunho_aberto)
    }
}

fn todas_as_condicoes(d: &Definicao) -> impl Iterator<Item = &Condicao> {
    d.detectar
        .todos
        .iter()
        .chain(&d.detectar.algum)
        .chain(&d.detectar.nenhum)
}

fn mimes_da(d: &Definicao) -> Vec<String> {
    let mut mimes: Vec<String> = todas_as_condicoes(d)
        .filter_map(|c| match c {
            Condicao::Mime(m) => Some(m.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    mimes.sort();
    mimes.dedup();
    mimes
}

/// As condições da detecção como texto: o que a pessoa procuraria ("comprovante de pix",
/// "image/*", o canal, o regex).
pub fn condicoes_em_texto(d: &Definicao) -> Vec<String> {
    todas_as_condicoes(d)
        .flat_map(|c| match c {
            Condicao::Contem(t)
            | Condicao::Regex(t)
            | Condicao::Descricao(t)
            | Condicao::Nome(t)
            | Condicao::Contraparte(t) => vec![t.clone()],
            Condicao::Mime(m) | Condicao::ContraparteDoc(m) => m.clone(),
            Condicao::Canal(c) => c.iter().map(|c| format!("{c:?}").to_lowercase()).collect(),
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// Filtros
// ---------------------------------------------------------------------------------------

/// As duas vistas da área Padrões: a lista (com a tabela de sempre) e o mapa em árvore.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbaDosPadroes {
    #[default]
    Lista,
    Mapa,
}

/// "Casam com o documento selecionado" (D5): os dois modos e o desligado.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasamComODocumento {
    /// Não filtra por documento.
    #[default]
    Todos,
    /// **Casam agora**: o motor lê o texto guardado do documento e fica quem reconhece.
    Agora,
    /// **Já casaram**: quem já casou com ele alguma vez, pela execução gravada
    /// ([`PadroesQueJaCasaram`]).
    JaCasaram,
}

impl CasamComODocumento {
    pub fn rotulo(self) -> &'static str {
        match self {
            Self::Todos => "não filtrar",
            Self::Agora => "casam agora",
            Self::JaCasaram => "já casaram",
        }
    }
}

/// Os filtros da lista de padrões. Combinam (E) e vão para as preferências da janela.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiltroDePadroes {
    /// Busca por nome, tipo documental, id e condições (sem diferenciar maiúsculas nem
    /// acentos; todas as palavras têm de aparecer).
    #[serde(default)]
    pub texto: String,
    #[serde(default)]
    pub estado: Option<Estado>,
    /// O padrão se aplica a este tipo de entrada (`aplica_a`).
    #[serde(default)]
    pub entrada: Option<TipoDeEntrada>,
    /// A detecção exige este tipo real (`Condicao::Mime`).
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub casam: CasamComODocumento,
}

impl FiltroDePadroes {
    /// Nenhum filtro ligado.
    pub fn limpo(&self) -> bool {
        self.texto.trim().is_empty()
            && self.estado.is_none()
            && self.entrada.is_none()
            && self.mime.is_none()
            && self.casam == CasamComODocumento::Todos
    }

    /// Quantos filtros estão ligados (o rótulo do botão "Limpar").
    pub fn ligados(&self) -> usize {
        usize::from(!self.texto.trim().is_empty())
            + usize::from(self.estado.is_some())
            + usize::from(self.entrada.is_some())
            + usize::from(self.mime.is_some())
            + usize::from(self.casam != CasamComODocumento::Todos)
    }
}

impl PadraoDaLista {
    fn feno(&self) -> String {
        let mut partes = vec![
            self.nome.clone(),
            self.id.clone(),
            self.tipo_documental.clone(),
            rotulo_do_estado(self.estado).to_owned(),
        ];
        partes.extend(self.condicoes.iter().cloned());
        normalizar(&partes.join("\n"))
    }

    fn casa_com_o_texto(&self, texto: &str) -> bool {
        let busca = normalizar(texto);
        if busca.is_empty() {
            return true;
        }
        let feno = self.feno();
        busca
            .split_whitespace()
            .all(|palavra| feno.contains(palavra))
    }
}

/// Os padrões que passam por todos os filtros, na ordem recebida.
///
/// `casam` é o conjunto de padrões que casam com o documento selecionado, quando o modo de
/// "casam com o documento" está ligado e há com o que comparar (a tela decide: sem documento,
/// ou sem a fonte do "já casaram", passa `None` e o filtro não age).
pub fn filtrar<'a>(
    padroes: &'a [PadraoDaLista],
    filtro: &FiltroDePadroes,
    casam: Option<&BTreeSet<IdDaRegra>>,
) -> Vec<&'a PadraoDaLista> {
    padroes
        .iter()
        .filter(|p| p.casa_com_o_texto(&filtro.texto))
        .filter(|p| filtro.estado.is_none_or(|e| p.tem_o_estado(e)))
        .filter(|p| filtro.entrada.is_none_or(|t| p.aplica_a.contains(&t)))
        .filter(|p| {
            filtro
                .mime
                .as_ref()
                .is_none_or(|m| p.mimes.iter().any(|x| x == m))
        })
        .filter(|p| casam.is_none_or(|c| c.contains(&p.id)))
        .collect()
}

/// Os tipos reais (MIME) que os padrões exigem, sem repetir, em ordem: o que o filtro oferece.
pub fn mimes_disponiveis(padroes: &[PadraoDaLista]) -> Vec<String> {
    padroes
        .iter()
        .flat_map(|p| p.mimes.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

// ---------------------------------------------------------------------------------------
// O mapa em árvore
// ---------------------------------------------------------------------------------------

/// Uma folha do mapa: um padrão e o que o põe nesta ordem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolhaDoMapa {
    pub id: IdDaRegra,
    pub nome: String,
    pub versao: u32,
    pub estado: Estado,
    pub prioridade: i32,
    pub especificidade: usize,
    /// Outros padrões **em vigor** do mesmo tipo de entrada com a mesma prioridade e
    /// especificidade: se casarem juntos e discordarem, o motor dá Ambíguo.
    pub empata_com: Vec<IdDaRegra>,
}

/// Os padrões de um tipo documental, em ordem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RamoDocumental {
    pub tipo_documental: String,
    pub padroes: Vec<FolhaDoMapa>,
}

/// Os tipos documentais de um tipo de entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RamoDeEntrada {
    pub tipo: TipoDeEntrada,
    pub documentais: Vec<RamoDocumental>,
}

/// O mapa: tipo de entrada, depois tipo documental, depois os padrões por prioridade e
/// especificidade (as maiores primeiro; no empate, pelo id). Um padrão que se aplica a dois
/// tipos de entrada aparece nos dois. Só leitura.
pub fn montar_mapa(padroes: &[&PadraoDaLista]) -> Vec<RamoDeEntrada> {
    let mut mapa = Vec::new();
    for tipo in TIPOS_DE_ENTRADA {
        let dele: Vec<&&PadraoDaLista> = padroes
            .iter()
            .filter(|p| p.aplica_a.contains(&tipo))
            .collect();
        if dele.is_empty() {
            continue;
        }
        let mut por_tipo: BTreeMap<&str, Vec<&PadraoDaLista>> = BTreeMap::new();
        for p in &dele {
            por_tipo.entry(&p.tipo_documental).or_default().push(p);
        }
        let documentais = por_tipo
            .into_iter()
            .map(|(tipo_documental, mut ps)| {
                ps.sort_by(|a, b| {
                    (b.prioridade, b.especificidade, &a.id).cmp(&(
                        a.prioridade,
                        a.especificidade,
                        &b.id,
                    ))
                });
                RamoDocumental {
                    tipo_documental: tipo_documental.to_owned(),
                    padroes: ps
                        .into_iter()
                        .map(|p| FolhaDoMapa {
                            id: p.id.clone(),
                            nome: p.nome.clone(),
                            versao: p.versao,
                            estado: p.estado,
                            prioridade: p.prioridade,
                            especificidade: p.especificidade,
                            empata_com: empata_com(p, &dele),
                        })
                        .collect(),
                }
            })
            .collect();
        mapa.push(RamoDeEntrada { tipo, documentais });
    }
    mapa
}

fn empata_com(p: &PadraoDaLista, do_mesmo_tipo_de_entrada: &[&&PadraoDaLista]) -> Vec<IdDaRegra> {
    if !vigente(p.estado) {
        return Vec::new();
    }
    do_mesmo_tipo_de_entrada
        .iter()
        .filter(|o| {
            o.id != p.id
                && vigente(o.estado)
                && (o.prioridade, o.especificidade) == (p.prioridade, p.especificidade)
        })
        .map(|o| o.id.clone())
        .collect()
}

// ---------------------------------------------------------------------------------------
// "Por que este padrão?"
// ---------------------------------------------------------------------------------------

/// A prioridade e a especificidade de uma versão: o que o motor compara.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PesoDaVersao {
    pub prioridade: i32,
    pub especificidade: usize,
}

impl PesoDaVersao {
    pub fn da(d: &Definicao) -> Self {
        Self {
            prioridade: d.prioridade,
            especificidade: d.especificidade(),
        }
    }
}

/// O peso de cada versão que a leitura cita, por `(padrão, versão)`.
pub type PesosDasVersoes = BTreeMap<(IdDaRegra, u32), PesoDaVersao>;

/// O papel de um padrão que casou no desfecho.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Papel {
    Venceu,
    /// Casou, mas outro prevaleceu (ou empatou e concordou).
    NaoPrevaleceu,
    /// Empatou na prioridade e na especificidade e discorda dos outros: nada foi aplicado.
    Discorda,
}

/// Um padrão que casou com a entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadraoQueCasou {
    pub regra: IdDaRegra,
    pub versao: u32,
    pub peso: Option<PesoDaVersao>,
    pub papel: Papel,
    /// O que o motor anotou na execução (só para quem não venceu).
    pub nota: Option<String>,
}

/// Um padrão que detectou o documento e falhou ao ler (só a categoria do erro).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadraoQueFalhou {
    pub regra: IdDaRegra,
    pub versao: u32,
    pub categoria: String,
}

/// O campo em que as candidatas de um Ambíguo discordam e o que cada uma leu nele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergencia {
    /// `tipo_documental` ou um campo crítico (`valor_centavos`, `data`...).
    pub campo: String,
    pub valores: Vec<(IdDaRegra, u32, String)>,
}

/// Como terminou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desfecho {
    /// Um padrão prevaleceu. `motivo` é o que o motor escreveu (`None` quando foi o único que
    /// casou).
    Venceu {
        regra: IdDaRegra,
        versao: u32,
        motivo: Option<String>,
    },
    /// Empate que discorda: nada foi aplicado e vai para revisão.
    Ambiguo {
        motivo: String,
        divergencia: Option<Divergencia>,
    },
    /// Nenhum padrão casou (o leitor nativo e a IA seguem).
    NenhumCasou,
}

/// "Por que este padrão?": tudo o que o cartão mostra, em dados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PorQue {
    pub tipo_da_entrada: Option<TipoDeEntrada>,
    pub mime: Option<String>,
    /// Quantos padrões se aplicam a este tipo de entrada (o motor só avalia esses).
    pub aplicam: usize,
    /// Os que casaram, na ordem do motor: maior prioridade, maior especificidade, menor id.
    pub casaram: Vec<PadraoQueCasou>,
    pub falharam: Vec<PadraoQueFalhou>,
    pub desfecho: Desfecho,
}

/// Os começos dos motivos que o motor escreve no vencedor para dizer por que prevaleceu.
const COMECOS_DO_MOTIVO_DO_VENCEDOR: [&str; 3] =
    ["prioridade ", "mais específic", "empate de prioridade"];

/// Monta o rastro "Por que este padrão?" da leitura do motor. `entrada` dá o tipo e o MIME;
/// `pesos` dá a prioridade e a especificidade de cada versão citada (quem não tiver aparece
/// sem os números).
pub fn montar_por_que(
    leitura: &Leitura,
    entrada: Option<&Entrada>,
    pesos: &PesosDasVersoes,
) -> PorQue {
    let peso = |r: &str, v: u32| pesos.get(&(r.to_owned(), v)).copied();
    let aplicam = leitura.execucoes.len();
    let (vencedora, candidatas_do_ambiguo): (Option<_>, &[_]) = match &leitura.decisao {
        Decisao::Reconhecido(r) => (Some(r.as_ref()), &[]),
        Decisao::Ambiguo { candidatas, .. } => (None, candidatas.as_slice()),
        Decisao::Nenhuma => (None, &[]),
    };
    let mut casaram: Vec<PadraoQueCasou> = leitura
        .execucoes
        .iter()
        .filter(|e| e.resultado == ResultadoDaExecucao::Reconhecido)
        .map(|e| {
            let papel = if vencedora.is_some_and(|v| v.regra == e.regra && v.versao == e.versao) {
                Papel::Venceu
            } else if candidatas_do_ambiguo
                .iter()
                .any(|c| c.regra == e.regra && c.versao == e.versao)
            {
                Papel::Discorda
            } else {
                Papel::NaoPrevaleceu
            };
            PadraoQueCasou {
                regra: e.regra.clone(),
                versao: e.versao,
                peso: peso(&e.regra, e.versao),
                papel,
                nota: (papel == Papel::NaoPrevaleceu)
                    .then(|| e.motivo.clone())
                    .flatten(),
            }
        })
        .collect();
    casaram.sort_by(|a, b| {
        let chave = |c: &PadraoQueCasou| c.peso.map(|p| (p.prioridade, p.especificidade));
        chave(b)
            .cmp(&chave(a))
            .then_with(|| (&a.regra, a.versao).cmp(&(&b.regra, b.versao)))
    });
    let falharam = leitura
        .execucoes
        .iter()
        .filter(|e| e.resultado == ResultadoDaExecucao::Erro)
        .map(|e| PadraoQueFalhou {
            regra: e.regra.clone(),
            versao: e.versao,
            categoria: e.motivo.clone().unwrap_or_else(|| "erro".to_owned()),
        })
        .collect();
    let desfecho = match &leitura.decisao {
        Decisao::Reconhecido(r) => Desfecho::Venceu {
            regra: r.regra.clone(),
            versao: r.versao,
            motivo: if casaram.len() > 1 {
                r.motivos
                    .iter()
                    .rev()
                    .find(|m| {
                        COMECOS_DO_MOTIVO_DO_VENCEDOR
                            .iter()
                            .any(|c| m.starts_with(c))
                    })
                    .cloned()
            } else {
                None
            },
        },
        Decisao::Ambiguo { candidatas, motivo } => Desfecho::Ambiguo {
            motivo: motivo.clone(),
            divergencia: campo_em_que_discordam(candidatas).map(|campo| Divergencia {
                campo: campo.to_owned(),
                valores: candidatas
                    .iter()
                    .map(|c| {
                        let lido = if campo == "tipo_documental" {
                            c.tipo_documental.clone()
                        } else {
                            c.campos
                                .get(campo)
                                .map_or_else(|| "não leu".to_owned(), |k| k.valor.to_string())
                        };
                        (c.regra.clone(), c.versao, lido)
                    })
                    .collect(),
            }),
        },
        Decisao::Nenhuma => Desfecho::NenhumCasou,
    };
    PorQue {
        tipo_da_entrada: entrada.map(|e| e.tipo),
        mime: entrada.and_then(|e| e.mime.clone()),
        aplicam,
        casaram,
        falharam,
        desfecho,
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_regras::catalogo::NovaVersao;
    use sbf_regras::motor::Motor;
    use sbf_regras::{Autor, Dicas};

    fn versao(regra: &str, n: u32, estado: Estado, d: Definicao) -> VersaoDeRegra {
        VersaoDeRegra::avulsa(
            NovaVersao {
                regra: regra.to_owned(),
                nome: regra.to_owned(),
                definicao: d,
                script: None,
                autor: Autor::Usuario,
                motivo: "teste".to_owned(),
            },
            n,
            estado,
        )
        .expect("versão válida")
    }

    fn definicao(tipo: &str, contem: &str, prioridade: i32) -> Definicao {
        let mut d = Definicao::simples(tipo, contem);
        d.prioridade = prioridade;
        d
    }

    fn linha(id: &str, estado: Estado, d: Definicao) -> PadraoDaLista {
        PadraoDaLista::montar(id, &format!("Padrão {id}"), &[versao(id, 1, estado, d)])
            .expect("linha")
    }

    fn entrada(texto: &str) -> Entrada {
        crate::regras::entrada_de_texto(texto)
    }

    // ---- a linha ----

    #[test]
    fn a_linha_representa_o_padrao_pela_versao_em_vigor_e_avisa_do_rascunho_mais_novo() {
        let v1 = versao("pix", 1, Estado::Desativada, definicao("pix", "pix", 0));
        let v2 = versao("pix", 2, Estado::Consolidada, definicao("pix-2", "pix", 5));
        let v3 = versao("pix", 3, Estado::Rascunho, definicao("pix-3", "pix", 9));
        let l = PadraoDaLista::montar("pix", "Pix", &[v1, v2, v3]).expect("linha");
        assert_eq!((l.versao, l.estado), (2, Estado::Consolidada));
        assert_eq!((l.tipo_documental.as_str(), l.prioridade), ("pix-2", 5));
        assert!(l.tem_rascunho_aberto);
        assert!(l.tem_o_estado(Estado::Rascunho));
        assert!(l.tem_o_estado(Estado::Consolidada));
        assert!(!l.tem_o_estado(Estado::Desativada));
    }

    #[test]
    fn sem_versao_em_vigor_a_ultima_representa() {
        let v1 = versao("a", 1, Estado::Desativada, definicao("t", "x", 0));
        let v2 = versao("a", 2, Estado::Desativada, definicao("t", "x", 0));
        let l = PadraoDaLista::montar("a", "A", &[v1, v2]).expect("linha");
        assert_eq!(l.versao, 2);
        assert!(!l.tem_rascunho_aberto);
        assert!(PadraoDaLista::montar("a", "A", &[]).is_none());
    }

    #[test]
    fn as_condicoes_e_os_mimes_saem_da_deteccao() {
        let mut d = definicao("recibo", "oficina", 0);
        d.detectar
            .todos
            .push(Condicao::Mime(vec!["image/*".into()]));
        d.detectar
            .algum
            .push(Condicao::Mime(vec!["application/pdf".into()]));
        d.detectar.nenhum.push(Condicao::Regex("estorno".into()));
        let l = linha("recibo", Estado::Ativa, d);
        assert_eq!(l.mimes, ["application/pdf", "image/*"]);
        assert!(l.condicoes.iter().any(|c| c == "oficina"));
        assert!(l.condicoes.iter().any(|c| c == "estorno"));
        assert_eq!(mimes_disponiveis(&[l]), ["application/pdf", "image/*"]);
    }

    // ---- filtros ----

    fn acervo() -> Vec<PadraoDaLista> {
        let mut recibo = definicao("recibo-de-oficina", "oficina mecânica", 0);
        recibo
            .detectar
            .todos
            .push(Condicao::Mime(vec!["image/*".into()]));
        let mut transacao = definicao("transacao-pix", "pix recebido", 0);
        transacao.aplica_a = vec![TipoDeEntrada::Registro];
        let mut aviso = definicao("aviso-de-vencimento", "vence amanhã", 0);
        aviso.aplica_a = vec![TipoDeEntrada::Mensagem, TipoDeEntrada::Documento];
        vec![
            linha(
                "pix",
                Estado::Consolidada,
                definicao("comprovante-pix", "comprovante de pix", 0),
            ),
            linha(
                "fatura",
                Estado::Rascunho,
                definicao("fatura-de-cartao", "fatura", 0),
            ),
            linha("recibo", Estado::Ativa, recibo),
            linha("transacao", Estado::Ativa, transacao),
            linha("aviso", Estado::Desativada, aviso),
        ]
    }

    fn ids(v: &[&PadraoDaLista]) -> Vec<String> {
        v.iter().map(|p| p.id.clone()).collect()
    }

    #[test]
    fn sem_filtro_passam_todos() {
        let a = acervo();
        assert!(FiltroDePadroes::default().limpo());
        assert_eq!(filtrar(&a, &FiltroDePadroes::default(), None).len(), 5);
    }

    #[test]
    fn o_texto_busca_nome_tipo_documental_e_condicoes_sem_acento() {
        let a = acervo();
        let busca = |t: &str| {
            ids(&filtrar(
                &a,
                &FiltroDePadroes {
                    texto: t.to_owned(),
                    ..Default::default()
                },
                None,
            ))
        };
        assert_eq!(busca("COMPROVANTE"), ["pix"]);
        assert_eq!(busca("fatura de cartao"), ["fatura"]);
        // Condição com acento na definição; a busca sem acento acha.
        assert_eq!(busca("mecanica"), ["recibo"]);
        assert_eq!(busca("vence amanha"), ["aviso"]);
        // O id também (o usuário o vê na lista).
        assert_eq!(busca("transacao"), ["transacao"]);
        assert!(busca("inexistente").is_empty());
        // Todas as palavras têm de aparecer.
        assert_eq!(busca("pix recebido"), ["transacao"]);
    }

    #[test]
    fn o_estado_filtra_e_o_rascunho_tambem_acha_quem_tem_rascunho_aberto() {
        let mut a = acervo();
        let v1 = versao("pix", 1, Estado::Consolidada, definicao("t", "pix", 0));
        let v2 = versao("pix", 2, Estado::Rascunho, definicao("t", "pix", 1));
        a[0] = PadraoDaLista::montar("pix", "Pix", &[v1, v2]).expect("linha");
        let por = |e| {
            ids(&filtrar(
                &a,
                &FiltroDePadroes {
                    estado: Some(e),
                    ..Default::default()
                },
                None,
            ))
        };
        assert_eq!(por(Estado::Rascunho), ["pix", "fatura"]);
        assert_eq!(por(Estado::Ativa), ["recibo", "transacao"]);
        assert_eq!(por(Estado::Consolidada), ["pix"]);
        assert_eq!(por(Estado::Desativada), ["aviso"]);
    }

    #[test]
    fn o_tipo_de_entrada_e_o_mime_filtram() {
        let a = acervo();
        let f = |entrada, mime: Option<&str>| {
            ids(&filtrar(
                &a,
                &FiltroDePadroes {
                    entrada,
                    mime: mime.map(str::to_owned),
                    ..Default::default()
                },
                None,
            ))
        };
        assert_eq!(f(Some(TipoDeEntrada::Registro), None), ["transacao"]);
        assert_eq!(f(Some(TipoDeEntrada::Mensagem), None), ["aviso"]);
        assert_eq!(
            f(Some(TipoDeEntrada::Documento), None),
            ["pix", "fatura", "recibo", "aviso"]
        );
        assert_eq!(f(None, Some("image/*")), ["recibo"]);
        assert!(f(None, Some("application/pdf")).is_empty());
    }

    #[test]
    fn casam_com_o_documento_restringe_ao_conjunto() {
        let a = acervo();
        let casam: BTreeSet<IdDaRegra> = ["pix".to_owned(), "aviso".to_owned()].into();
        let f = FiltroDePadroes::default();
        assert_eq!(ids(&filtrar(&a, &f, Some(&casam))), ["pix", "aviso"]);
        // Conjunto vazio: ninguém casa (diferente de "sem conjunto", que não filtra).
        assert!(filtrar(&a, &f, Some(&BTreeSet::new())).is_empty());
    }

    #[test]
    fn os_filtros_combinam_com_e() {
        let a = acervo();
        let casam: BTreeSet<IdDaRegra> =
            ["pix".to_owned(), "aviso".to_owned(), "recibo".to_owned()].into();
        let f = FiltroDePadroes {
            texto: "vence".to_owned(),
            estado: Some(Estado::Desativada),
            entrada: Some(TipoDeEntrada::Mensagem),
            mime: None,
            casam: CasamComODocumento::Agora,
        };
        assert_eq!(f.ligados(), 4);
        assert_eq!(ids(&filtrar(&a, &f, Some(&casam))), ["aviso"]);
        let so_ativo = FiltroDePadroes {
            estado: Some(Estado::Ativa),
            ..f
        };
        assert!(filtrar(&a, &so_ativo, Some(&casam)).is_empty());
    }

    #[test]
    fn o_filtro_vai_e_volta_do_json_e_valor_desconhecido_nao_derruba() {
        let f = FiltroDePadroes {
            texto: "pix".to_owned(),
            estado: Some(Estado::Ativa),
            entrada: Some(TipoDeEntrada::Registro),
            mime: Some("image/*".to_owned()),
            casam: CasamComODocumento::JaCasaram,
        };
        let json = serde_json::to_string(&f).expect("json");
        assert_eq!(
            serde_json::from_str::<FiltroDePadroes>(&json).expect("lê"),
            f
        );
        assert_eq!(
            serde_json::from_str::<FiltroDePadroes>("{}").expect("vazio"),
            FiltroDePadroes::default()
        );
    }

    // ---- o mapa ----

    #[test]
    fn o_mapa_agrupa_por_entrada_e_tipo_documental_e_ordena_por_prioridade_e_especificidade() {
        let mut mais_especifico = definicao("comprovante-pix", "comprovante de pix", 0);
        mais_especifico
            .detectar
            .todos
            .push(Condicao::Contem("banco exemplo".into()));
        let a = [
            linha(
                "pix-geral",
                Estado::Ativa,
                definicao("comprovante-pix", "pix", 0),
            ),
            linha(
                "pix-prioritario",
                Estado::Ativa,
                definicao("comprovante-pix", "pix", 10),
            ),
            linha("pix-especifico", Estado::Consolidada, mais_especifico),
            linha("fatura", Estado::Rascunho, definicao("fatura", "fatura", 0)),
            linha("aviso", Estado::Ativa, {
                let mut d = definicao("aviso", "vence", 0);
                d.aplica_a = vec![TipoDeEntrada::Mensagem, TipoDeEntrada::Documento];
                d
            }),
        ];
        let refs: Vec<&PadraoDaLista> = a.iter().collect();
        let mapa = montar_mapa(&refs);
        assert_eq!(
            mapa.iter().map(|r| r.tipo).collect::<Vec<_>>(),
            [TipoDeEntrada::Documento, TipoDeEntrada::Mensagem]
        );
        let documento = &mapa[0];
        assert_eq!(
            documento
                .documentais
                .iter()
                .map(|d| d.tipo_documental.as_str())
                .collect::<Vec<_>>(),
            ["aviso", "comprovante-pix", "fatura"]
        );
        let pix = documento
            .documentais
            .iter()
            .find(|d| d.tipo_documental == "comprovante-pix")
            .expect("pix");
        assert_eq!(
            pix.padroes
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["pix-prioritario", "pix-especifico", "pix-geral"]
        );
        // O aviso se aplica a documento e a mensagem: aparece nos dois.
        assert_eq!(mapa[1].documentais[0].padroes[0].id, "aviso");
    }

    #[test]
    fn o_mapa_aponta_o_empate_so_entre_padroes_em_vigor_do_mesmo_tipo_de_entrada() {
        let a = [
            linha("a", Estado::Ativa, definicao("t1", "x", 0)),
            linha("b", Estado::Consolidada, definicao("t2", "y", 0)),
            linha("c", Estado::Rascunho, definicao("t3", "z", 0)),
            linha("d", Estado::Ativa, definicao("t4", "w", 7)),
        ];
        let refs: Vec<&PadraoDaLista> = a.iter().collect();
        let mapa = montar_mapa(&refs);
        let folha = |id: &str| {
            mapa[0]
                .documentais
                .iter()
                .flat_map(|d| &d.padroes)
                .find(|p| p.id == id)
                .cloned()
                .expect("folha")
        };
        assert_eq!(folha("a").empata_com, ["b"]);
        assert_eq!(folha("b").empata_com, ["a"]);
        // Rascunho não roda; prioridade diferente não empata.
        assert!(folha("c").empata_com.is_empty());
        assert!(folha("d").empata_com.is_empty());
    }

    #[test]
    fn mapa_vazio_sem_padroes() {
        assert!(montar_mapa(&[]).is_empty());
    }

    // ---- "Por que este padrão?" ----

    fn leitura(versoes: Vec<VersaoDeRegra>, texto: &str) -> (Leitura, PesosDasVersoes, Entrada) {
        let pesos = versoes
            .iter()
            .map(|v| ((v.regra.clone(), v.versao), PesoDaVersao::da(&v.definicao)))
            .collect();
        let e = entrada(texto);
        (Motor::novo(versoes, Vec::new(), None).ler(&e), pesos, e)
    }

    #[test]
    fn um_so_padrao_vence_sem_motivo_de_desempate() {
        let (l, pesos, e) = leitura(
            vec![versao(
                "pix",
                1,
                Estado::Ativa,
                definicao("pix", "comprovante", 0),
            )],
            "Comprovante de Pix",
        );
        let q = montar_por_que(&l, Some(&e), &pesos);
        assert_eq!(q.tipo_da_entrada, Some(TipoDeEntrada::Documento));
        assert_eq!(q.aplicam, 1);
        assert_eq!(q.casaram.len(), 1);
        assert_eq!(q.casaram[0].papel, Papel::Venceu);
        assert_eq!(
            q.desfecho,
            Desfecho::Venceu {
                regra: "pix".into(),
                versao: 1,
                motivo: None
            }
        );
    }

    #[test]
    fn a_prioridade_decide_e_o_motivo_vem_do_motor() {
        let (l, pesos, e) = leitura(
            vec![
                versao(
                    "geral",
                    1,
                    Estado::Ativa,
                    definicao("geral", "comprovante", 10),
                ),
                versao(
                    "especifica",
                    1,
                    Estado::Ativa,
                    definicao("esp", "comprovante", 0),
                ),
                versao("outra", 1, Estado::Ativa, definicao("outra", "boleto", 0)),
            ],
            "Comprovante de Pix",
        );
        let q = montar_por_que(&l, Some(&e), &pesos);
        // O motor só avalia os que se aplicam ao tipo da entrada; aqui os três se aplicam.
        assert_eq!(q.aplicam, 3);
        assert_eq!(
            q.casaram
                .iter()
                .map(|c| c.regra.as_str())
                .collect::<Vec<_>>(),
            ["geral", "especifica"]
        );
        assert_eq!(q.casaram[0].papel, Papel::Venceu);
        assert_eq!(
            q.casaram[0].peso,
            Some(PesoDaVersao {
                prioridade: 10,
                especificidade: 1
            })
        );
        assert_eq!(q.casaram[1].papel, Papel::NaoPrevaleceu);
        assert_eq!(
            q.casaram[1].nota.as_deref(),
            Some("não prevaleceu: venceu o padrão geral")
        );
        match &q.desfecho {
            Desfecho::Venceu { regra, motivo, .. } => {
                assert_eq!(regra, "geral");
                assert_eq!(
                    motivo.as_deref(),
                    Some("prioridade 10 > 0 do padrão especifica")
                );
            }
            outro => panic!("{outro:?}"),
        }
    }

    #[test]
    fn a_especificidade_desempata_e_aparece_no_motivo() {
        let mut especifica = definicao("t", "comprovante", 0);
        especifica
            .detectar
            .todos
            .push(Condicao::Contem("pix".into()));
        let (l, pesos, e) = leitura(
            vec![
                versao(
                    "a-geral",
                    1,
                    Estado::Ativa,
                    definicao("t", "comprovante", 0),
                ),
                versao("b-especifica", 1, Estado::Ativa, especifica),
            ],
            "Comprovante de Pix",
        );
        let q = montar_por_que(&l, Some(&e), &pesos);
        match &q.desfecho {
            Desfecho::Venceu { regra, motivo, .. } => {
                assert_eq!(regra, "b-especifica");
                assert!(
                    motivo
                        .as_deref()
                        .is_some_and(|m| m.starts_with("mais específico"))
                );
            }
            outro => panic!("{outro:?}"),
        }
    }

    #[test]
    fn o_ambiguo_mostra_os_dois_e_o_campo_em_que_discordam() {
        let (l, pesos, e) = leitura(
            vec![
                versao(
                    "regra-a",
                    1,
                    Estado::Ativa,
                    definicao("tipo-a", "comprovante", 0),
                ),
                versao("regra-b", 2, Estado::Ativa, definicao("tipo-b", "pix", 0)),
            ],
            "Comprovante de Pix",
        );
        let q = montar_por_que(&l, Some(&e), &pesos);
        assert_eq!(q.casaram.len(), 2);
        assert!(q.casaram.iter().all(|c| c.papel == Papel::Discorda));
        match q.desfecho {
            Desfecho::Ambiguo {
                motivo,
                divergencia,
            } => {
                assert!(motivo.contains("discordam em tipo_documental"), "{motivo}");
                let d = divergencia.expect("divergência");
                assert_eq!(d.campo, "tipo_documental");
                assert_eq!(
                    d.valores,
                    [
                        ("regra-a".to_owned(), 1, "tipo-a".to_owned()),
                        ("regra-b".to_owned(), 2, "tipo-b".to_owned())
                    ]
                );
            }
            outro => panic!("{outro:?}"),
        }
    }

    #[test]
    fn um_empate_que_concorda_vence_o_menor_id_e_o_outro_nao_prevalece() {
        let (l, pesos, e) = leitura(
            vec![
                versao("a", 1, Estado::Ativa, definicao("t", "comprovante", 0)),
                versao("b", 1, Estado::Ativa, definicao("t", "pix", 0)),
            ],
            "Comprovante de Pix",
        );
        let q = montar_por_que(&l, Some(&e), &pesos);
        assert_eq!(q.casaram[0].papel, Papel::Venceu);
        assert_eq!(q.casaram[1].papel, Papel::NaoPrevaleceu);
        match q.desfecho {
            Desfecho::Venceu { motivo, .. } => {
                assert!(motivo.is_some_and(|m| m.contains("menor id")));
            }
            outro => panic!("{outro:?}"),
        }
    }

    #[test]
    fn nenhum_casou_e_erro_de_regex_aparece_em_falharam() {
        let (l, pesos, e) = leitura(
            vec![versao("a", 1, Estado::Ativa, definicao("t", "boleto", 0))],
            "Comprovante de Pix",
        );
        let q = montar_por_que(&l, Some(&e), &pesos);
        assert_eq!((q.aplicam, q.casaram.len()), (1, 0));
        assert_eq!(q.desfecho, Desfecho::NenhumCasou);

        let q = montar_por_que(
            &Leitura {
                entrada: "x".into(),
                decisao: Decisao::Nenhuma,
                execucoes: vec![sbf_regras::Execucao {
                    regra: "ruim".into(),
                    versao: 3,
                    resultado: ResultadoDaExecucao::Erro,
                    motivo: Some("regex".into()),
                    detalhe: None,
                    duracao_us: 0,
                }],
            },
            None,
            &PesosDasVersoes::new(),
        );
        assert_eq!(q.tipo_da_entrada, None);
        assert_eq!(
            q.falharam,
            [PadraoQueFalhou {
                regra: "ruim".into(),
                versao: 3,
                categoria: "regex".into()
            }]
        );
    }

    #[test]
    fn so_se_avaliam_os_padroes_do_tipo_da_entrada() {
        let mut registro = definicao("transacao", "pix", 0);
        registro.aplica_a = vec![TipoDeEntrada::Registro];
        let versoes = vec![
            versao("doc", 1, Estado::Ativa, definicao("comprovante", "pix", 0)),
            versao("reg", 1, Estado::Ativa, registro),
        ];
        let pesos: PesosDasVersoes = versoes
            .iter()
            .map(|v| ((v.regra.clone(), v.versao), PesoDaVersao::da(&v.definicao)))
            .collect();
        let mut e = entrada("pix recebido");
        e.tipo = TipoDeEntrada::Registro;
        e.dicas = Dicas::default();
        let l = Motor::novo(versoes, Vec::new(), None).ler(&e);
        let q = montar_por_que(&l, Some(&e), &pesos);
        assert_eq!(q.tipo_da_entrada, Some(TipoDeEntrada::Registro));
        assert_eq!(q.aplicam, 1);
        assert_eq!(q.casaram[0].regra, "reg");
    }
}
