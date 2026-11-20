//! O painel do Reconhecimento falando com as **regras de reconhecimento** (spec 13): os
//! nomes legíveis dos campos, a leitura do que o usuário digita para corrigir um valor, o
//! diálogo "Testar com este documento" e o formulário "Criar regra a partir daqui" (D7).
//!
//! Aqui mora o estado e a lógica, sem egui desenhando nada (o desenho está em
//! [`crate::reconhecimento_regras_ui`]). Nada aqui ativa, consolida, desativa ou reverte
//! uma regra (D14): "Salvar" grava um **rascunho**, e ativar continua sendo um clique na
//! página da regra.

use std::collections::BTreeMap;

use sbf_planejamento::Data;
use sbf_regras::rascunho::Correcao;
use sbf_regras::{DeOnde, Entrada, Leitura, Valor};

use crate::janela::Janela;
use crate::regras::{FonteDasRegras, OpcaoDeTeste, RascunhoMontado};

/// O rótulo amigável de um campo extraído ("valor_centavos" vira "Valor").
pub fn rotulo_do_campo(nome: &str) -> String {
    match nome {
        "valor_centavos" => "Valor".to_owned(),
        "data" => "Data".to_owned(),
        "vencimento" => "Vencimento".to_owned(),
        "favorecido" => "Favorecido".to_owned(),
        "autenticacao" => "Autenticação".to_owned(),
        "competencia" => "Competência".to_owned(),
        "liquido_centavos" => "Líquido".to_owned(),
        "tipo_de_transferencia" => "Tipo de transferência".to_owned(),
        "irrf_retido_centavos" => "IRRF retido".to_owned(),
        "irrf_estimado_centavos" => "IRRF estimado".to_owned(),
        "irrf_diferenca_centavos" => "Diferença do IRRF".to_owned(),
        "base_irrf_centavos" => "Base do IRRF".to_owned(),
        outro => {
            let limpo = outro.strip_suffix("_centavos").unwrap_or(outro);
            let mut letras = limpo.replace('_', " ").chars().collect::<Vec<_>>();
            if let Some(primeira) = letras.first_mut() {
                *primeira = primeira.to_uppercase().next().unwrap_or(*primeira);
            }
            letras.into_iter().collect()
        }
    }
}

/// De onde veio um campo, em português (AC-LUA-08). Com o texto do documento à mão, o
/// trecho aparece (cortado em 60 caracteres).
pub fn descrever_de_onde(de: &DeOnde, texto: Option<&str>) -> String {
    match de {
        DeOnde::Trecho { inicio, fim } => {
            let trecho = texto
                .and_then(|t| t.get(*inicio..*fim))
                .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|t| !t.is_empty());
            match trecho {
                Some(t) => {
                    let curto: String = t.chars().take(60).collect();
                    let reticencias = if t.chars().count() > 60 { "…" } else { "" };
                    format!("trecho do texto «{curto}{reticencias}»")
                }
                None => format!("trecho do texto (posições {inicio} a {fim})"),
            }
        }
        DeOnde::Dica { nome } => format!("dica da origem: {nome}"),
        DeOnde::Leitor { leitor } => format!("leitor {leitor}"),
        DeOnde::Script { a_partir_de } => {
            if a_partir_de.is_empty() {
                "script da regra".to_owned()
            } else {
                format!(
                    "script da regra, a partir de {}",
                    a_partir_de
                        .iter()
                        .map(|n| rotulo_do_campo(n))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Ler o que o usuário digita
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tipo {
    Texto,
    Centavos,
    Data,
    Competencia,
    Inteiro,
    Booleano,
}

fn tipo_do_valor(v: &Valor) -> Tipo {
    match v {
        Valor::Texto(_) => Tipo::Texto,
        Valor::Centavos(_) => Tipo::Centavos,
        Valor::Data(_) => Tipo::Data,
        Valor::Competencia(_) => Tipo::Competencia,
        Valor::Inteiro(_) => Tipo::Inteiro,
        Valor::Booleano(_) => Tipo::Booleano,
    }
}

fn tipo_pelo_nome(nome: &str) -> Option<Tipo> {
    if nome.ends_with("_centavos") || nome == "valor" {
        Some(Tipo::Centavos)
    } else if nome == "data" || nome == "vencimento" || nome.starts_with("data_") {
        Some(Tipo::Data)
    } else if nome == "competencia" {
        Some(Tipo::Competencia)
    } else if matches!(nome, "favorecido" | "autenticacao") {
        Some(Tipo::Texto)
    } else {
        None
    }
}

fn so_digitos(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn ler_data(t: &str) -> Option<Valor> {
    let iso = match t.split('/').collect::<Vec<_>>().as_slice() {
        [d, m, a] if so_digitos(d) && so_digitos(m) && a.len() == 4 && so_digitos(a) => {
            format!("{a}-{m:0>2}-{d:0>2}")
        }
        _ if t.len() == 10 && t.as_bytes().get(4) == Some(&b'-') => t.to_owned(),
        _ => return None,
    };
    Data::de_iso(&iso).map(|_| Valor::Data(iso))
}

fn ler_competencia(t: &str) -> Option<Valor> {
    let (m, a) = match t.split_once('/') {
        Some((m, a)) => (m, a),
        None => {
            let (a, m) = t.split_once('-')?;
            (m, a)
        }
    };
    if !(so_digitos(m) && a.len() == 4 && so_digitos(a)) {
        return None;
    }
    let mes: u32 = m.parse().ok()?;
    (1..=12)
        .contains(&mes)
        .then(|| Valor::Competencia(format!("{a}-{mes:02}")))
}

fn ler_centavos(t: &str) -> Option<Valor> {
    let mut t = t.trim();
    let negativo = t.starts_with('-');
    t = t.trim_start_matches('-').trim();
    t = t.strip_prefix("R$").unwrap_or(t).trim();
    let limpo = t.replace('.', "");
    let (inteiro, frac) = match limpo.split_once(',') {
        Some((i, f)) => (i, f),
        None => (limpo.as_str(), ""),
    };
    if !so_digitos(inteiro) || frac.len() > 2 || (!frac.is_empty() && !so_digitos(frac)) {
        return None;
    }
    let reais: i64 = inteiro.parse().ok()?;
    let cent: i64 = if frac.is_empty() {
        0
    } else {
        format!("{frac:0<2}").parse().ok()?
    };
    let total = reais.checked_mul(100)?.checked_add(cent)?;
    Some(Valor::Centavos(if negativo { -total } else { total }))
}

/// Lê o texto digitado como o valor de um campo. O tipo vem do valor que o campo tinha
/// (`modelo`), senão do nome do campo, senão do formato do texto. `None` quando não dá para
/// entender (a janela avisa) ou o texto é vazio.
pub fn valor_do_texto(nome: &str, modelo: Option<&Valor>, texto: &str) -> Option<Valor> {
    let t = texto.trim();
    if t.is_empty() {
        return None;
    }
    let tipo = modelo
        .map(tipo_do_valor)
        .or_else(|| tipo_pelo_nome(nome))
        .unwrap_or_else(|| {
            if ler_data(t).is_some() && t.contains('/') {
                Tipo::Data
            } else if ler_competencia(t).is_some() && t.contains('/') {
                Tipo::Competencia
            } else if t.contains(',') && ler_centavos(t).is_some() {
                Tipo::Centavos
            } else {
                Tipo::Texto
            }
        });
    match tipo {
        Tipo::Texto => Some(Valor::Texto(t.to_owned())),
        Tipo::Centavos => ler_centavos(t),
        Tipo::Data => ler_data(t),
        Tipo::Competencia => ler_competencia(t),
        Tipo::Inteiro => t.parse().ok().map(Valor::Inteiro),
        Tipo::Booleano => match t.to_lowercase().as_str() {
            "sim" | "true" => Some(Valor::Booleano(true)),
            "não" | "nao" | "false" => Some(Valor::Booleano(false)),
            _ => None,
        },
    }
}

// ---------------------------------------------------------------------------------------
// Testar com este documento
// ---------------------------------------------------------------------------------------

/// O diálogo "Testar com este documento". Só lê: nada é gravado até "Guardar como exemplo".
#[derive(Debug)]
pub struct Teste {
    /// O cartão de onde veio (para o título).
    pub documento: String,
    pub entrada: Entrada,
    pub opcoes: Vec<OpcaoDeTeste>,
    /// O índice em `opcoes` da versão escolhida.
    pub escolhida: usize,
    /// O que a versão escolhida leu, ou o erro.
    pub leitura: Option<Result<Leitura, String>>,
    pub guardando: bool,
    pub nome_do_exemplo: String,
    pub mensagem: Option<(bool, String)>,
}

impl Teste {
    /// A versão escolhida.
    pub fn opcao(&self) -> Option<&OpcaoDeTeste> {
        self.opcoes.get(self.escolhida)
    }
}

// ---------------------------------------------------------------------------------------
// Criar regra a partir daqui
// ---------------------------------------------------------------------------------------

/// Uma linha do formulário: o campo (nome técnico), o rótulo e o que o usuário digitou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeCampo {
    pub nome: String,
    pub texto: String,
    /// O nome é do usuário (campo livre).
    pub livre: bool,
}

/// O formulário "Criar regra a partir daqui" (D7).
#[derive(Debug)]
pub struct Criacao {
    pub documento: String,
    pub entrada: Entrada,
    pub nome: String,
    pub tipo_documental: String,
    pub linhas: Vec<LinhaDeCampo>,
    /// O rascunho montado (a prévia, com os motivos), ainda sem gravar.
    pub montado: Option<RascunhoMontado>,
    pub erro: Option<String>,
}

/// Os campos de sempre, na ordem do formulário.
pub const CAMPOS_USUAIS: [&str; 5] = [
    "valor_centavos",
    "data",
    "vencimento",
    "favorecido",
    "autenticacao",
];

impl Criacao {
    fn nova(
        documento: String,
        entrada: Entrada,
        tipo_documental: String,
        iniciais: &BTreeMap<String, String>,
    ) -> Self {
        let mut linhas: Vec<LinhaDeCampo> = CAMPOS_USUAIS
            .iter()
            .map(|n| LinhaDeCampo {
                nome: (*n).to_owned(),
                texto: iniciais.get(*n).cloned().unwrap_or_default(),
                livre: false,
            })
            .collect();
        for (nome, texto) in iniciais {
            if !CAMPOS_USUAIS.contains(&nome.as_str()) {
                linhas.push(LinhaDeCampo {
                    nome: nome.clone(),
                    texto: texto.clone(),
                    livre: true,
                });
            }
        }
        if !linhas.iter().any(|l| l.livre) {
            linhas.push(LinhaDeCampo {
                nome: String::new(),
                texto: String::new(),
                livre: true,
            });
        }
        let nome = documento
            .rsplit_once('.')
            .map_or(documento.as_str(), |(n, _)| n)
            .to_owned();
        Self {
            documento,
            entrada,
            nome,
            tipo_documental,
            linhas,
            montado: None,
            erro: None,
        }
    }

    /// O que o usuário preencheu, lido como valores. O erro diz qual campo não entendi.
    ///
    /// # Errors
    /// O texto do campo que não deu para ler.
    pub fn correcao(&self) -> Result<Correcao, String> {
        let nome = self.nome.trim();
        if nome.is_empty() {
            return Err("dê um nome à regra".to_owned());
        }
        let tipo = self.tipo_documental.trim();
        if tipo.is_empty() {
            return Err("diga o tipo documental (comprovante-pix, fatura-de-cartao...)".to_owned());
        }
        let mut campos = BTreeMap::new();
        for l in &self.linhas {
            let (n, t) = (l.nome.trim(), l.texto.trim());
            if t.is_empty() {
                continue;
            }
            if n.is_empty() {
                return Err("um campo livre tem valor mas não tem nome".to_owned());
            }
            let Some(v) = valor_do_texto(n, None, t) else {
                return Err(format!(
                    "não entendi o valor de «{}»: use R$ 1.250,00, 12/10/2026 ou texto",
                    rotulo_do_campo(n)
                ));
            };
            campos.insert(n.to_owned(), v);
        }
        if campos.is_empty() {
            return Err("preencha pelo menos um campo que a regra deve ler".to_owned());
        }
        Ok(Correcao {
            regra: None,
            nome: nome.to_owned(),
            tipo_documental: tipo.to_owned(),
            campos,
        })
    }
}

// ---------------------------------------------------------------------------------------
// A janela
// ---------------------------------------------------------------------------------------

impl Janela {
    /// O documento contra o qual a área Padrões confere "casam com o documento" (spec 15, D5):
    /// o cartão da linha escolhida da Caixa de entrada ou, sem ele, o cartão mais recente do
    /// Reconhecimento que guarda a entrada lida pelo motor.
    pub(crate) fn documento_de_referencia_dos_padroes(
        &self,
    ) -> Option<crate::regras_ui::DocumentoDeReferencia> {
        let da_caixa = self
            .caixa_de_entrada
            .selecionada
            .is_some()
            .then_some(self.reconhecimento.da_caixa)
            .flatten();
        let indices = da_caixa
            .into_iter()
            .chain((0..self.reconhecimento.cartoes.len()).rev());
        for i in indices {
            let Some(c) = self.reconhecimento.cartoes.get(i) else {
                continue;
            };
            if let Some(r) = c.resultado()
                && let Some(entrada) = &r.entrada
            {
                return Some(crate::regras_ui::DocumentoDeReferencia {
                    rotulo: c.nome.clone(),
                    entrada: entrada.clone(),
                });
            }
        }
        None
    }

    fn fonte_das_regras(&self) -> Option<FonteDasRegras> {
        self.regras.fonte().cloned()
    }

    /// Abre "Testar com este documento" para o cartão `cartao`.
    pub fn abrir_teste(&mut self, cartao: usize) {
        let Some(fonte) = self.fonte_das_regras() else {
            self.mensagem_das_regras(true, "O catálogo de regras não está ligado a esta janela.");
            return;
        };
        let Some(c) = self.reconhecimento.cartoes.get(cartao) else {
            return;
        };
        let Some(entrada) = c.resultado().and_then(|r| r.entrada.clone()) else {
            return;
        };
        let da_regra = c.regra_reconhecida();
        let documento = c.nome.clone();
        let opcoes = match fonte.versoes_para_testar() {
            Ok(o) => o,
            Err(e) => {
                self.mensagem_das_regras(true, &e);
                return;
            }
        };
        let escolhida = da_regra
            .and_then(|(regra, versao)| {
                opcoes
                    .iter()
                    .position(|o| o.regra == regra && o.versao == versao)
            })
            .unwrap_or(0);
        self.reconhecimento.teste = Some(Teste {
            documento,
            entrada,
            opcoes,
            escolhida,
            leitura: None,
            guardando: false,
            nome_do_exemplo: String::new(),
            mensagem: None,
        });
        self.rodar_teste();
    }

    /// Lê o documento com a versão escolhida (só ela no motor). Não grava nada.
    pub fn rodar_teste(&mut self) {
        let Some(fonte) = self.fonte_das_regras() else {
            return;
        };
        let Some(t) = self.reconhecimento.teste.as_mut() else {
            return;
        };
        t.mensagem = None;
        t.leitura = t
            .opcao()
            .map(|o| fonte.testar_documento(&o.regra, o.versao, &t.entrada));
    }

    /// "Guardar como exemplo": grava o documento na regra, com o que a versão leu.
    pub fn guardar_teste_como_exemplo(&mut self) {
        let Some(fonte) = self.fonte_das_regras() else {
            return;
        };
        let Some(t) = self.reconhecimento.teste.as_mut() else {
            return;
        };
        let Some(o) = t.opcao().cloned() else {
            return;
        };
        t.mensagem = Some(
            match fonte.guardar_como_exemplo(&o.regra, o.versao, &t.nome_do_exemplo, &t.entrada) {
                Ok(_) => {
                    t.guardando = false;
                    (
                        false,
                        format!(
                            "Exemplo «{}» guardado na regra {}.",
                            t.nome_do_exemplo.trim(),
                            o.nome
                        ),
                    )
                }
                Err(e) => (true, e),
            },
        );
    }

    pub fn fechar_teste(&mut self) {
        self.reconhecimento.teste = None;
    }

    /// Abre "Criar regra a partir daqui" para o cartão `cartao`, já com o que a regra leu e
    /// o que o usuário corrigiu no cartão.
    pub fn abrir_criacao(&mut self, cartao: usize) {
        if self.regras.fonte().is_none() {
            self.mensagem_das_regras(true, "O catálogo de regras não está ligado a esta janela.");
            return;
        }
        let Some(c) = self.reconhecimento.cartoes.get(cartao) else {
            return;
        };
        let Some(r) = c.resultado() else { return };
        let Some(entrada) = r.entrada.clone() else {
            return;
        };
        let (tipo, mut iniciais) = c.campos_para_a_criacao();
        for (n, t) in &c.edicao {
            iniciais.insert(n.clone(), t.clone());
        }
        self.reconhecimento.criacao = Some(Criacao::nova(c.nome.clone(), entrada, tipo, &iniciais));
    }

    /// Monta o rascunho e mostra a prévia com os motivos. Não grava.
    pub fn montar_criacao(&mut self) {
        let Some(fonte) = self.fonte_das_regras() else {
            return;
        };
        let Some(c) = self.reconhecimento.criacao.as_mut() else {
            return;
        };
        c.montado = None;
        c.erro = None;
        match c
            .correcao()
            .and_then(|k| fonte.montar_rascunho(&c.entrada, &k))
        {
            Ok(m) => c.montado = Some(m),
            Err(e) => c.erro = Some(e),
        }
    }

    /// Salva o rascunho (autor `Correcao`) e o exemplo, fecha o formulário e abre a página da
    /// regra na área Regras. Devolve o id da regra, para quem navega.
    pub fn salvar_criacao(&mut self) -> Option<String> {
        let fonte = self.fonte_das_regras()?;
        let c = self.reconhecimento.criacao.as_mut()?;
        let montado = c.montado.clone()?;
        match fonte.salvar_rascunho_montado(&montado, &c.entrada) {
            Ok(v) => {
                let motivos = if montado.motivos.is_empty() {
                    String::new()
                } else {
                    format!(" Ficou de fora: {}.", montado.motivos.join("; "))
                };
                self.reconhecimento.criacao = None;
                self.regras.abrir(&v.regra);
                self.mensagem_das_regras(
                    false,
                    &format!(
                        "Rascunho criado a partir da correção (versão {}). Ele não reconhece nada até você ativar.{motivos}",
                        v.versao
                    ),
                );
                Some(v.regra)
            }
            Err(e) => {
                c.erro = Some(e);
                None
            }
        }
    }

    pub fn fechar_criacao(&mut self) {
        self.reconhecimento.criacao = None;
    }

    fn mensagem_das_regras(&mut self, erro: bool, texto: &str) {
        self.regras.avisar(erro, texto);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn os_rotulos_dos_campos_sao_amigaveis() {
        assert_eq!(rotulo_do_campo("valor_centavos"), "Valor");
        assert_eq!(rotulo_do_campo("autenticacao"), "Autenticação");
        assert_eq!(
            rotulo_do_campo("numero_do_documento"),
            "Numero do documento"
        );
        assert_eq!(rotulo_do_campo("multa_centavos"), "Multa");
    }

    #[test]
    fn o_texto_digitado_vira_valor_pelo_tipo_do_campo() {
        let v = |n: &str, t: &str| valor_do_texto(n, None, t);
        assert_eq!(
            v("valor_centavos", "R$ 1.250,00"),
            Some(Valor::Centavos(125_000))
        );
        assert_eq!(v("valor_centavos", "187,5"), Some(Valor::Centavos(18_750)));
        assert_eq!(v("valor_centavos", "30"), Some(Valor::Centavos(3_000)));
        assert_eq!(v("valor_centavos", "1.2x"), None);
        assert_eq!(
            v("data", "12/10/2026"),
            Some(Valor::Data("2026-10-12".into()))
        );
        assert_eq!(
            v("data", "2026-10-12"),
            Some(Valor::Data("2026-10-12".into()))
        );
        assert_eq!(v("data", "31/02/2026"), None);
        assert_eq!(
            v("vencimento", "5/11/2026"),
            Some(Valor::Data("2026-11-05".into()))
        );
        assert_eq!(
            v("competencia", "10/2026"),
            Some(Valor::Competencia("2026-10".into()))
        );
        assert_eq!(v("competencia", "13/2026"), None);
        assert_eq!(
            v("favorecido", " Fulano de Tal "),
            Some(Valor::Texto("Fulano de Tal".into()))
        );
        // Campo livre: o formato do texto decide.
        assert_eq!(v("multa", "12,00"), Some(Valor::Centavos(1_200)));
        assert_eq!(
            v("emitido", "01/02/2026"),
            Some(Valor::Data("2026-02-01".into()))
        );
        assert_eq!(v("codigo", "ABC123"), Some(Valor::Texto("ABC123".into())));
        assert_eq!(v("data", "  "), None);
    }

    #[test]
    fn o_modelo_do_valor_manda_mais_que_o_nome() {
        let modelo = Valor::Data("2026-10-12".into());
        assert_eq!(
            valor_do_texto("qualquer", Some(&modelo), "13/10/2026"),
            Some(Valor::Data("2026-10-13".into()))
        );
        assert_eq!(valor_do_texto("qualquer", Some(&modelo), "texto"), None);
    }

    #[test]
    fn de_onde_mostra_o_trecho_a_dica_o_leitor_e_o_script() {
        let texto = "Banco Exemplo\nValor:   R$ 187,50\n";
        let de = DeOnde::Trecho {
            inicio: 14,
            fim: 32,
        };
        assert_eq!(
            descrever_de_onde(&de, Some(texto)),
            "trecho do texto «Valor: R$ 187,50»"
        );
        assert_eq!(
            descrever_de_onde(&de, None),
            "trecho do texto (posições 14 a 32)"
        );
        // Posição fora do texto não quebra.
        assert!(
            descrever_de_onde(
                &DeOnde::Trecho {
                    inicio: 900,
                    fim: 910
                },
                Some(texto)
            )
            .contains("posições")
        );
        assert_eq!(
            descrever_de_onde(
                &DeOnde::Dica {
                    nome: "descricao".into()
                },
                None
            ),
            "dica da origem: descricao"
        );
        assert_eq!(
            descrever_de_onde(
                &DeOnde::Leitor {
                    leitor: "contracheque".into()
                },
                None
            ),
            "leitor contracheque"
        );
        assert_eq!(
            descrever_de_onde(
                &DeOnde::Script {
                    a_partir_de: vec!["valor_centavos".into(), "data".into()]
                },
                None
            ),
            "script da regra, a partir de Valor, Data"
        );
    }
}

#[cfg(test)]
mod testes_do_painel {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use sbf_planejamento::{Centavos, Competencia, Conciliacao, IdOcorrencia};
    use sbf_reconhecimento::regras::entrada_do_documento;
    use sbf_reconhecimento::{
        ReconhecedorFicticio, Reconhecido, Resultado, Sugestao, ocorrencia_do_mes,
    };
    use sbf_regras::catalogo::{Esperado, EstadoDaProposta};
    use sbf_regras::motor::Motor;
    use sbf_regras::{Autor, Canal, Decisao, Estado};

    use super::*;
    use crate::planejamento_ficticio::{HOJE_DA_DEMONSTRACAO, PlanejamentoFicticio};
    use crate::reconhecimento::SituacaoDoCartao;
    use crate::reconhecimento_ficticio::{NOME_DO_AMBIGUO, NOME_DO_PIX, ligar_na_demonstracao};
    use crate::tema;

    const PIX: &str = "comprovante-pix-banco-exemplo";
    const ENERGIA: &str = "conta-de-energia-distribuidora-exemplo";

    fn janela() -> (Janela, FonteDasRegras) {
        let mut j = Janela::nova();
        j.hoje = HOJE_DA_DEMONSTRACAO.to_owned();
        let hoje = Data::de_iso(HOJE_DA_DEMONSTRACAO).expect("data");
        j.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje)));
        let f = FonteDasRegras::ficticia(None).expect("catálogo");
        j.regras.ligar(f.clone());
        ligar_na_demonstracao(&mut j);
        (j, f)
    }

    fn soltar(j: &mut Janela, nome: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("dir");
        let p: PathBuf = dir.path().join(nome);
        std::fs::write(&p, b"ficticio").expect("escrever");
        j.soltar_arquivos(vec![p], None);
        let limite = Instant::now() + Duration::from_secs(10);
        while j.tique_do_reconhecimento(None) {
            assert!(Instant::now() < limite, "o reconhecimento não terminou");
            std::thread::sleep(Duration::from_millis(5));
        }
        dir
    }

    fn estado_da_proposta(f: &FonteDasRegras, chave: &str) -> EstadoDaProposta {
        f.catalogo
            .lock()
            .expect("trava")
            .propostas(None)
            .expect("propostas")
            .into_iter()
            .find(|p| p.chave == chave)
            .expect("a proposta existe")
            .estado
    }

    fn aceitas_seguidas(f: &FonteDasRegras, regra: &str, versao: u32) -> u32 {
        f.versoes(regra)
            .expect("versões")
            .into_iter()
            .find(|v| v.versao == versao)
            .expect("versão")
            .aceitas_seguidas
    }

    fn chave_da_prova(j: &Janela) -> String {
        let r = j.reconhecimento.cartoes[0].resultado().expect("resultado");
        match &r.sugestoes[0] {
            Sugestao::ProvarRegistro { chave, .. } => chave.clone(),
            outra => panic!("esperava ProvarRegistro, veio {outra:?}"),
        }
    }

    fn documentos_do_alvo(j: &Janela) -> usize {
        j.planejamento
            .as_ref()
            .expect("planejamento")
            .0
            .documentos(&ocorrencia_do_mes(
                "rec_energia",
                Competencia { ano: 2026, mes: 11 },
            ))
            .expect("documentos")
            .iter()
            .filter(|d| !d.desvinculado)
            .count()
    }

    // ---- o cartão ----

    #[test]
    fn o_pix_chega_pela_regra_com_campos_rastro_e_a_sugestao_de_provar() {
        let (mut j, _) = janela();
        let _d = soltar(&mut j, "qualquer-pix.pdf");
        let c = &j.reconhecimento.cartoes[0];
        let r = c.resultado().expect("resultado");
        let Reconhecido::PelaRegra {
            reconhecimento: rec,
        } = &r.reconhecido
        else {
            panic!("devia vir pela regra: {:?}", r.reconhecido);
        };
        assert_eq!(rec.regra, PIX);
        assert_eq!(rec.tipo_documental, "comprovante-pix");
        assert_eq!(rec.campos["valor_centavos"].valor.to_string(), "R$ 187,50");
        assert_eq!(rec.campos["data"].valor.to_string(), "12/11/2026");
        // O nome da regra vem do catálogo, não o id.
        assert_eq!(
            c.nomes_das_regras.get(PIX).map(String::as_str),
            Some("Comprovante Pix · Banco Exemplo")
        );
        // O Rastro: o campo veio de um trecho do texto, e o trecho se lê.
        let texto = r.entrada.as_ref().and_then(|e| e.texto.as_deref());
        let origem = descrever_de_onde(&rec.campos["valor_centavos"].de_onde, texto);
        assert!(
            origem.contains("trecho do texto") && origem.contains("187,50"),
            "{origem}"
        );
        assert!(matches!(
            r.sugestoes[0],
            Sugestao::ProvarRegistro {
                automatica: false,
                ..
            }
        ));
        assert!(r.leitura.is_some() && r.entrada.is_some());
    }

    #[test]
    fn o_ambiguo_nao_aplica_nada_e_nao_oferece_sugestao() {
        let (mut j, f) = janela();
        let antes = f
            .catalogo
            .lock()
            .expect("t")
            .propostas(None)
            .expect("p")
            .len();
        let _d = soltar(&mut j, "comprovante-agendado.pdf");
        let r = j.reconhecimento.cartoes[0].resultado().expect("resultado");
        let Reconhecido::Ambiguo { candidatas, motivo } = &r.reconhecido else {
            panic!("devia ser ambíguo: {:?}", r.reconhecido);
        };
        assert_eq!(candidatas.len(), 2);
        assert!(!motivo.is_empty());
        assert!(r.sugestoes.is_empty());
        // Nenhuma proposta nasce de um caso ambíguo.
        assert_eq!(
            f.catalogo
                .lock()
                .expect("t")
                .propostas(None)
                .expect("p")
                .len(),
            antes
        );
    }

    // ---- o retorno (D4) ----

    #[test]
    fn confirmar_a_sugestao_da_regra_registra_aceito() {
        let (mut j, f) = janela();
        let _d = soltar(&mut j, NOME_DO_PIX);
        let chave = chave_da_prova(&j);
        let antes = aceitas_seguidas(&f, PIX, 2);
        assert_eq!(estado_da_proposta(&f, &chave), EstadoDaProposta::Aberta);
        let docs = documentos_do_alvo(&j);
        j.confirmar_sugestao(0, 0);
        assert_eq!(estado_da_proposta(&f, &chave), EstadoDaProposta::Aceita);
        assert_eq!(aceitas_seguidas(&f, PIX, 2), antes + 1);
        // O comprovante se ligou à ocorrência que já tinha o débito.
        assert_eq!(documentos_do_alvo(&j), docs + 1);
        // Confirmar de novo não repete nada.
        j.confirmar_sugestao(0, 0);
        assert_eq!(aceitas_seguidas(&f, PIX, 2), antes + 1);
        assert!(j.reconhecimento.cartoes[0].aviso_de_retorno.is_none());
    }

    #[test]
    fn descartar_a_sugestao_registra_rejeitado_e_zera_a_sequencia() {
        let (mut j, f) = janela();
        let _d = soltar(&mut j, NOME_DO_PIX);
        let chave = chave_da_prova(&j);
        assert!(aceitas_seguidas(&f, PIX, 2) >= 5);
        let docs = documentos_do_alvo(&j);
        j.ignorar_sugestao(0, 0);
        assert_eq!(estado_da_proposta(&f, &chave), EstadoDaProposta::Recusada);
        assert_eq!(aceitas_seguidas(&f, PIX, 2), 0);
        // Descartar não escreve no planejamento.
        assert_eq!(documentos_do_alvo(&j), docs);
        // A regra continua em vigor: rejeitar uma proposta não desativa nada.
        assert_eq!(
            f.resumos()
                .expect("r")
                .iter()
                .find(|r| r.id == PIX)
                .expect("pix")
                .estado,
            Some(Estado::Ativa)
        );
    }

    #[test]
    fn aplicar_depois_de_corrigir_um_valor_registra_corrigido() {
        let (mut j, f) = janela();
        let _d = soltar(&mut j, NOME_DO_PIX);
        let chave = chave_da_prova(&j);
        assert!(j.reconhecimento.cartoes[0].corrigidos().is_empty());
        j.alternar_correcao(0);
        let c = &mut j.reconhecimento.cartoes[0];
        assert!(c.corrigindo);
        // As caixas começam com o que a regra leu: nada corrigido ainda.
        assert_eq!(c.edicao["valor_centavos"], "R$ 187,50");
        assert!(c.corrigidos().is_empty());
        c.edicao
            .insert("favorecido".into(), "Ciclano de Tal".into());
        assert_eq!(c.corrigidos(), vec!["favorecido".to_owned()]);
        j.confirmar_sugestao(0, 0);
        // Aceita, mas como correção: a sequência zera em vez de somar.
        assert_eq!(estado_da_proposta(&f, &chave), EstadoDaProposta::Aceita);
        assert_eq!(aceitas_seguidas(&f, PIX, 2), 0);
    }

    #[test]
    fn mexer_na_caixa_e_voltar_ao_valor_original_nao_conta_como_correcao() {
        let (mut j, f) = janela();
        let _d = soltar(&mut j, NOME_DO_PIX);
        j.alternar_correcao(0);
        let c = &mut j.reconhecimento.cartoes[0];
        // O mesmo valor escrito de outro jeito não corrige nada.
        c.edicao.insert("valor_centavos".into(), "187,5".into());
        assert!(c.corrigidos().is_empty());
        let antes = aceitas_seguidas(&f, PIX, 2);
        j.confirmar_sugestao(0, 0);
        assert_eq!(aceitas_seguidas(&f, PIX, 2), antes + 1);
    }

    fn janela_com_conciliacao_automatica() -> (Janela, FonteDasRegras, String) {
        let (mut j, f) = janela();
        let texto = "Distribuidora Exemplo de Energia\nConta de energia elétrica\nTotal a pagar: R$ 187,32\nVencimento: 20/11/2026";
        let sha = crate::demonstracao::hash_do_nome("energia.pdf");
        let entrada = entrada_do_documento(
            &sha,
            "energia.pdf",
            "application/pdf",
            Some(texto),
            Canal::Arrastado,
            None,
        );
        let (rec, leitura) = {
            let mut c = f.catalogo.lock().expect("t");
            let motor = Motor::das_vigentes(&c, None).expect("motor");
            let leitura = motor.ler(&entrada);
            let Decisao::Reconhecido(rec) = leitura.decisao.clone() else {
                panic!("a energia devia ser reconhecida");
            };
            assert_eq!(rec.estado, Estado::Consolidada);
            c.registrar_proposta(&rec, &entrada.id).expect("proposta");
            (rec, leitura)
        };
        let sugestao = Sugestao::ConciliarOcorrencia {
            rotulo: "Conciliar a Energia · 11/2026 com o débito de 18/11".to_owned(),
            conciliacao: Conciliacao {
                ocorrencia: IdOcorrencia {
                    recorrente: "rec_energia".to_owned(),
                    competencia: Competencia { ano: 2026, mes: 11 },
                    parcela: None,
                },
                movimento: "pluggy:txn_energia_ficticia".to_owned(),
                valor: Centavos(18_732),
                data: Data {
                    ano: 2026,
                    mes: 11,
                    dia: 18,
                },
                confirmada: false,
            },
            regra: (rec.regra.clone(), rec.versao),
            chave: rec.chave.clone(),
            provas: 3,
            automatica: true,
        };
        let chave = rec.chave.clone();
        let resultado = Resultado {
            sha256: sha,
            nome: "energia.pdf".to_owned(),
            ja_existia: false,
            texto: sbf_reconhecimento::EstadoDoTexto::Extraido,
            reconhecido: Reconhecido::PelaRegra {
                reconhecimento: rec,
            },
            sugestoes: vec![sugestao],
            avisos: Vec::new(),
            entrada: Some(entrada),
            leitura: Some(leitura),
        };
        j.ligar_reconhecedor(Arc::new(ReconhecedorFicticio::novo(Ok(resultado))));
        (j, f, chave)
    }

    #[test]
    fn desfazer_a_automatica_registra_desfeito_e_rebaixa_a_regra() {
        let (mut j, f, chave) = janela_com_conciliacao_automatica();
        let _d = soltar(&mut j, "energia.pdf");
        let c = &j.reconhecimento.cartoes[0];
        assert!(
            c.automaticas.contains(&0),
            "as três provas conciliam sozinhas: {:?}",
            c.erro_de_sugestao
        );
        // O próprio rótulo da automática cita a regra pelo nome.
        let sug = &c.resultado().expect("r").sugestoes[0];
        assert!(
            crate::reconhecimento_regras_ui::texto_da_automatica(sug, &c.nomes_das_regras)
                .contains("Conta de energia · Distribuidora Exemplo")
        );
        assert_eq!(
            f.resumos()
                .expect("r")
                .iter()
                .find(|r| r.id == ENERGIA)
                .expect("e")
                .estado,
            Some(Estado::Consolidada)
        );
        j.desfazer_automatica(0, 0);
        assert_eq!(estado_da_proposta(&f, &chave), EstadoDaProposta::Desfeita);
        // A regra volta a ser ativa: só propõe, até o usuário consolidar de novo.
        assert_eq!(
            f.resumos()
                .expect("r")
                .iter()
                .find(|r| r.id == ENERGIA)
                .expect("e")
                .estado,
            Some(Estado::Ativa)
        );
        assert!(!j.reconhecimento.cartoes[0].automaticas.contains(&0));
        assert!(j.reconhecimento.cartoes[0].aviso_de_retorno.is_none());
    }

    #[test]
    fn sem_catalogo_o_cartao_funciona_sem_registrar_nada() {
        let mut j = Janela::nova();
        let hoje = Data::de_iso(HOJE_DA_DEMONSTRACAO).expect("data");
        j.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje)));
        ligar_na_demonstracao(&mut j);
        // Sem regras, o nome do Pix cai no contracheque de sempre.
        let _d = soltar(&mut j, NOME_DO_PIX);
        let r = j.reconhecimento.cartoes[0].resultado().expect("r");
        assert!(matches!(r.reconhecido, Reconhecido::Contracheque { .. }));
        j.ignorar_sugestao(0, 0);
        assert!(j.reconhecimento.cartoes[0].aviso_de_retorno.is_none());
    }

    // ---- Testar com este documento ----

    #[test]
    fn testar_com_este_documento_le_com_a_versao_escolhida_sem_gravar() {
        let (mut j, f) = janela();
        let _d = soltar(&mut j, NOME_DO_PIX);
        let exemplos = f.exemplos(PIX).expect("e").len();
        let versoes = f.versoes(PIX).expect("v").len();
        let propostas = f
            .catalogo
            .lock()
            .expect("t")
            .propostas(None)
            .expect("p")
            .len();
        j.abrir_teste(0);
        let t = j.reconhecimento.teste.as_ref().expect("diálogo");
        // Começa na versão que reconheceu o documento.
        let o = t.opcao().expect("opção");
        assert_eq!((o.regra.as_str(), o.versao), (PIX, 2));
        // As versões oferecidas incluem rascunhos (a fatura é só rascunho).
        assert!(t.opcoes.iter().any(|o| o.estado == Estado::Rascunho));
        let Some(Ok(l)) = &t.leitura else {
            panic!("devia ler")
        };
        let Decisao::Reconhecido(r) = &l.decisao else {
            panic!("devia reconhecer")
        };
        assert_eq!(r.campos["favorecido"].valor.to_string(), "Fulano de Tal");
        // A versão 1 não lê o favorecido.
        let v1 = t
            .opcoes
            .iter()
            .position(|o| o.regra == PIX && o.versao == 1)
            .expect("v1");
        j.reconhecimento.teste.as_mut().expect("t").escolhida = v1;
        j.rodar_teste();
        let Some(Ok(l)) = &j.reconhecimento.teste.as_ref().expect("t").leitura else {
            panic!()
        };
        let Decisao::Reconhecido(r) = &l.decisao else {
            panic!("devia reconhecer")
        };
        assert!(!r.campos.contains_key("favorecido"));
        // Um rascunho que não se aplica diz "nenhuma".
        let fatura = j
            .reconhecimento
            .teste
            .as_ref()
            .expect("t")
            .opcoes
            .iter()
            .position(|o| o.regra == "fatura-cartao-banco-exemplo")
            .expect("fatura");
        j.reconhecimento.teste.as_mut().expect("t").escolhida = fatura;
        j.rodar_teste();
        let Some(Ok(l)) = &j.reconhecimento.teste.as_ref().expect("t").leitura else {
            panic!()
        };
        assert!(matches!(l.decisao, Decisao::Nenhuma));
        // Nada foi gravado: nem exemplo, nem versão, nem proposta.
        assert_eq!(f.exemplos(PIX).expect("e").len(), exemplos);
        assert_eq!(f.versoes(PIX).expect("v").len(), versoes);
        assert_eq!(
            f.catalogo
                .lock()
                .expect("t")
                .propostas(None)
                .expect("p")
                .len(),
            propostas
        );
    }

    #[test]
    fn guardar_como_exemplo_grava_o_que_a_versao_leu() {
        let (mut j, f) = janela();
        let _d = soltar(&mut j, NOME_DO_PIX);
        j.abrir_teste(0);
        let antes = f.exemplos(PIX).expect("e").len();
        // Sem nome, não guarda.
        j.guardar_teste_como_exemplo();
        assert!(matches!(
            &j.reconhecimento.teste.as_ref().expect("t").mensagem,
            Some((true, _))
        ));
        assert_eq!(antes, f.exemplos(PIX).expect("e").len());
        j.reconhecimento.teste.as_mut().expect("t").nome_do_exemplo =
            "Pix de R$ 187,50 para Fulano".to_owned();
        j.guardar_teste_como_exemplo();
        let depois = f.exemplos(PIX).expect("e");
        assert_eq!(depois.len(), antes + 1);
        let ex = depois
            .iter()
            .find(|e| e.nome.starts_with("Pix de R$ 187,50"))
            .expect("exemplo");
        let Esperado::Reconhece {
            tipo_documental,
            campos,
        } = &ex.esperado
        else {
            panic!("devia esperar o reconhecimento");
        };
        assert_eq!(tipo_documental, "comprovante-pix");
        assert_eq!(campos["valor_centavos"], Valor::Centavos(18_750));
        assert_eq!(campos["favorecido"], Valor::Texto("Fulano de Tal".into()));
        // O exemplo passa na versão de onde saiu.
        assert!(f.testar(PIX, 2).expect("t").aprovado());
        // E com uma versão que não se aplica, guarda "não se aplica".
        let fatura = j
            .reconhecimento
            .teste
            .as_ref()
            .expect("t")
            .opcoes
            .iter()
            .position(|o| o.regra == "fatura-cartao-banco-exemplo")
            .expect("f");
        let t3 = j.reconhecimento.teste.as_mut().expect("t");
        t3.escolhida = fatura;
        t3.nome_do_exemplo = "Pix não é fatura (do cartão)".to_owned();
        j.guardar_teste_como_exemplo();
        let ex = f
            .exemplos("fatura-cartao-banco-exemplo")
            .expect("e")
            .into_iter()
            .find(|e| e.nome.starts_with("Pix não é fatura (do"))
            .expect("ex");
        assert_eq!(ex.esperado, Esperado::NaoSeAplica);
    }

    // ---- Criar regra a partir daqui ----

    #[test]
    fn criar_regra_monta_o_rascunho_mostra_os_motivos_e_so_salva_rascunho() {
        let (mut j, f) = janela();
        let _d = soltar(&mut j, NOME_DO_PIX);
        j.abrir_criacao(0);
        {
            let c = j.reconhecimento.criacao.as_mut().expect("form");
            // Já vem com o que a regra leu.
            let valor = c
                .linhas
                .iter()
                .find(|l| l.nome == "valor_centavos")
                .expect("valor");
            assert_eq!(valor.texto, "R$ 187,50");
            c.nome = "Pix para pessoas · Banco Exemplo".to_owned();
            c.tipo_documental = "comprovante-pix".to_owned();
            c.linhas
                .iter_mut()
                .find(|l| l.nome == "autenticacao")
                .expect("a")
                .texto = "D7K2-9QX1-EXEMPLO".to_owned();
            // Um campo livre que não existe no texto: vira motivo, não erro.
            let livre = c.linhas.iter_mut().find(|l| l.livre).expect("livre");
            livre.nome = "protocolo".to_owned();
            livre.texto = "999888777".to_owned();
        }
        j.montar_criacao();
        let c = j.reconhecimento.criacao.as_ref().expect("form");
        assert!(c.erro.is_none(), "{:?}", c.erro);
        let m = c.montado.as_ref().expect("rascunho montado");
        assert_eq!(m.nova.regra, "pix-para-pessoas-banco-exemplo");
        assert_eq!(m.nova.autor, Autor::Correcao);
        assert!(
            m.motivos.iter().any(|x| x.contains("protocolo")),
            "{:?}",
            m.motivos
        );
        // Montar não grava: a regra ainda não existe.
        assert!(!f.resumos().expect("r").iter().any(|r| r.id == m.nova.regra));
        let id = m.nova.regra.clone();
        let vigente_antes = f
            .resumos()
            .expect("r")
            .iter()
            .filter(|r| r.estado.is_some())
            .count();

        assert_eq!(j.salvar_criacao().as_deref(), Some(id.as_str()));
        assert!(j.reconhecimento.criacao.is_none());
        // A página da regra abre na área Regras, com o aviso do que ficou de fora.
        assert_eq!(j.regras.regra_aberta(), Some(id.as_str()));
        let versoes = f.versoes(&id).expect("versões");
        assert_eq!(versoes.len(), 1);
        assert_eq!(versoes[0].autor, Autor::Correcao);
        // D14: criar não ativa nada. Continua rascunho e fora de vigor.
        assert_eq!(versoes[0].estado, Estado::Rascunho);
        assert_eq!(
            f.resumos()
                .expect("r")
                .iter()
                .filter(|r| r.estado.is_some())
                .count(),
            vigente_antes
        );
        // O documento virou o primeiro exemplo, e ele passa.
        assert_eq!(f.exemplos(&id).expect("e").len(), 1);
        assert!(f.testar(&id, 1).expect("t").aprovado());
    }

    #[test]
    fn criar_regra_recusa_valor_que_nao_entende_e_nome_vazio() {
        let (mut j, f) = janela();
        let _d = soltar(&mut j, NOME_DO_PIX);
        j.abrir_criacao(0);
        let antes = f.resumos().expect("r").len();
        j.reconhecimento.criacao.as_mut().expect("f").nome = String::new();
        j.montar_criacao();
        assert!(
            j.reconhecimento
                .criacao
                .as_ref()
                .expect("f")
                .erro
                .as_deref()
                .is_some_and(|e| e.contains("nome"))
        );
        {
            let c = j.reconhecimento.criacao.as_mut().expect("f");
            c.nome = "Outra".to_owned();
            c.linhas
                .iter_mut()
                .find(|l| l.nome == "data")
                .expect("d")
                .texto = "ontem".to_owned();
        }
        j.montar_criacao();
        let c = j.reconhecimento.criacao.as_ref().expect("f");
        assert!(c.montado.is_none());
        assert!(
            c.erro.as_deref().is_some_and(|e| e.contains("Data")),
            "{:?}",
            c.erro
        );
        // Salvar sem montar não faz nada.
        assert_eq!(j.salvar_criacao(), None);
        assert_eq!(f.resumos().expect("r").len(), antes);
    }

    #[test]
    fn criar_regra_sem_texto_no_documento_avisa() {
        let (mut j, _) = janela();
        let _d = soltar(&mut j, NOME_DO_PIX);
        j.abrir_criacao(0);
        let c = j.reconhecimento.criacao.as_mut().expect("f");
        c.entrada.texto = None;
        c.nome = "Sem texto".to_owned();
        j.montar_criacao();
        let c = j.reconhecimento.criacao.as_ref().expect("f");
        assert!(c.montado.is_none() && c.erro.is_some());
    }

    #[test]
    fn os_botoes_so_existem_com_a_entrada_e_o_catalogo() {
        let (mut j, _) = janela();
        let _d = soltar(&mut j, "contracheque.pdf");
        // O contracheque da demonstração não traz entrada de regras: nada abre.
        j.abrir_teste(0);
        j.abrir_criacao(0);
        assert!(j.reconhecimento.teste.is_none() && j.reconhecimento.criacao.is_none());
    }

    // ---- a tela ----

    fn quadro(ctx: &egui::Context, j: &mut Janela) {
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 1400.0),
            )),
            ..egui::RawInput::default()
        };
        let mut saida = ctx.run_ui(entrada, |ui| {
            j.reconhecimento_ui(ui);
        });
        saida.textures_delta.clear();
    }

    #[test]
    fn a_tela_desenha_o_pix_o_ambiguo_e_os_dialogos_sem_quebrar() {
        let ctx = egui::Context::default();
        tema::instalar_fontes(&ctx);
        tema::aplicar(&ctx, "claro");
        let (mut j, _) = janela();
        let _d = soltar(&mut j, NOME_DO_PIX);
        let _e = soltar(&mut j, NOME_DO_AMBIGUO);
        quadro(&ctx, &mut j);
        j.alternar_correcao(0);
        quadro(&ctx, &mut j);
        j.abrir_teste(0);
        quadro(&ctx, &mut j);
        j.reconhecimento.teste.as_mut().expect("t").guardando = true;
        quadro(&ctx, &mut j);
        j.fechar_teste();
        j.abrir_criacao(0);
        quadro(&ctx, &mut j);
        j.montar_criacao();
        quadro(&ctx, &mut j);
        j.abrir_teste(1);
        quadro(&ctx, &mut j);
    }

    // ---- `exibir` e a linha da regra no cartão ----

    /// Todo o texto que o quadro pintou.
    fn textos_pintados(ctx: &egui::Context, j: &mut Janela) -> Vec<String> {
        textos_numa_tela_de(ctx, j, 1400.0)
    }

    /// Como [`textos_pintados`], numa tela mais alta (o que passa do fim da tela não é pintado).
    fn textos_numa_tela_de(ctx: &egui::Context, j: &mut Janela, altura: f32) -> Vec<String> {
        fn colher(forma: &egui::Shape, fora: &mut Vec<String>) {
            match forma {
                egui::Shape::Text(t) => fora.push(t.galley.text().to_owned()),
                egui::Shape::Vec(v) => v.iter().for_each(|f| colher(f, fora)),
                _ => {}
            }
        }
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, altura),
            )),
            ..egui::RawInput::default()
        };
        let mut saida = ctx.run_ui(entrada, |ui| {
            j.reconhecimento_ui(ui);
        });
        saida.textures_delta.clear();
        let mut textos = Vec::new();
        for c in &saida.shapes {
            colher(&c.shape, &mut textos);
        }
        textos
    }

    fn contem(textos: &[String], trecho: &str) -> bool {
        textos.iter().any(|t| t.contains(trecho))
    }

    fn ctx_pronto() -> egui::Context {
        let ctx = egui::Context::default();
        tema::instalar_fontes(&ctx);
        tema::aplicar(&ctx, "claro");
        ctx
    }

    fn leitura_da_regra(mut rec: sbf_regras::Reconhecimento) -> sbf_regras::Leitura {
        rec.leitor = None;
        sbf_regras::Leitura {
            entrada: "doc-ficticio".to_owned(),
            decisao: Decisao::Reconhecido(Box::new(rec)),
            execucoes: Vec::new(),
        }
    }

    #[test]
    fn o_rotulo_da_regra_traz_nome_e_versao() {
        assert_eq!(
            crate::reconhecimento_regras_ui::rotulo_da_regra("Contracheque", 3),
            "Regra Contracheque · versão 3"
        );
    }

    #[test]
    fn o_cartao_do_contracheque_desenha_a_regra_e_as_linhas_de_exibir() {
        let ctx = ctx_pronto();
        let (mut j, _) = janela();
        let _p = soltar(&mut j, NOME_DO_PIX);
        // Qualquer reconhecimento serve de molde para a regra que "leu" o contracheque.
        let mut rec = match &j.reconhecimento.cartoes[0]
            .resultado()
            .expect("r")
            .reconhecido
        {
            Reconhecido::PelaRegra { reconhecimento } => (**reconhecimento).clone(),
            outro => panic!("esperava PelaRegra, veio {outro:?}"),
        };
        rec.regra = "contracheque".to_owned();
        rec.versao = 3;
        rec.exibir = vec![
            "IRRF conferido ✓ · tabela irrf-teste".to_owned(),
            "⚠ IRRF diverge da tabela irrf-teste".to_owned(),
        ];
        let _c = soltar(&mut j, "contracheque.pdf");
        {
            let c = &mut j.reconhecimento.cartoes[1];
            c.nomes_das_regras
                .insert("contracheque".to_owned(), "Contracheque".to_owned());
            let SituacaoDoCartao::Pronto(r) = &mut c.situacao else {
                panic!("o contracheque deveria estar pronto");
            };
            assert!(matches!(r.reconhecido, Reconhecido::Contracheque { .. }));
            r.leitura = Some(leitura_da_regra(rec));
        }
        let textos = textos_pintados(&ctx, &mut j);
        assert!(
            contem(&textos, "Regra Contracheque · versão 3"),
            "{textos:?}"
        );
        assert!(contem(&textos, "IRRF conferido ✓ · tabela irrf-teste"));
        assert!(contem(&textos, "⚠ IRRF diverge da tabela irrf-teste"));

        // Sem a regra na leitura, o cartão do contracheque segue como sempre.
        let SituacaoDoCartao::Pronto(r) = &mut j.reconhecimento.cartoes[1].situacao else {
            panic!("pronto");
        };
        r.leitura = None;
        let textos = textos_pintados(&ctx, &mut j);
        assert!(!contem(&textos, "Regra Contracheque"));
        assert!(!contem(&textos, "IRRF conferido"));
    }

    #[test]
    fn o_cartao_pela_regra_desenha_as_linhas_de_exibir() {
        let ctx = ctx_pronto();
        let (mut j, _) = janela();
        let _p = soltar(&mut j, NOME_DO_PIX);
        let SituacaoDoCartao::Pronto(r) = &mut j.reconhecimento.cartoes[0].situacao else {
            panic!("pronto");
        };
        let Reconhecido::PelaRegra { reconhecimento } = &mut r.reconhecido else {
            panic!("esperava PelaRegra");
        };
        reconhecimento.exibir = vec!["linha curta do script".to_owned()];
        let textos = textos_pintados(&ctx, &mut j);
        assert!(contem(&textos, "linha curta do script"), "{textos:?}");
        assert!(contem(&textos, "Reconhecido pela regra"));
        assert!(contem(&textos, "· versão"));
    }

    #[test]
    fn a_tela_da_nova_regra_cria_o_rascunho() {
        let (mut j, f) = janela();
        let ctx = egui::Context::default();
        tema::instalar_fontes(&ctx);
        tema::aplicar(&ctx, "claro");
        j.regras.abrir_nova_regra();
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            ..egui::RawInput::default()
        };
        let mut saida = ctx.run_ui(entrada, |ui| j.regras.area(ui));
        saida.textures_delta.clear();
        assert!(f.criar_regra("Maiúscula", "x", "t").is_err());
        assert!(f.criar_regra("-comeca", "x", "t").is_err());
        assert!(f.criar_regra("ok-id", " ", "t").is_err());
        assert!(f.criar_regra("ok-id", "Nome", " ").is_err());
        assert!(
            f.criar_regra(PIX, "Duplicada", "t")
                .unwrap_err()
                .contains("já existe")
        );
        f.criar_regra("boleto-banco-exemplo", "Boleto · Banco Exemplo", "boleto")
            .expect("cria");
        let v = f.versoes("boleto-banco-exemplo").expect("v");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].estado, Estado::Rascunho);
        assert_eq!(v[0].definicao.tipo_documental, "boleto");
        assert_eq!(v[0].definicao.detectar.todos.len(), 1);
    }

    // ---- "Por que este padrão?" (spec 15, D6) ----

    #[test]
    fn por_que_este_padrao_mostra_o_vencedor_e_o_ambiguo_numa_secao_recolhivel() {
        let ctx = ctx_pronto();
        // Sem a animação de abrir, o corpo da seção aparece inteiro no primeiro quadro.
        ctx.all_styles_mut(|s| s.animation_time = 0.0);
        let (mut j, _) = janela();
        let _a = soltar(&mut j, NOME_DO_PIX);
        let _b = soltar(&mut j, NOME_DO_AMBIGUO);
        // Recolhida: só o título.
        let textos = textos_numa_tela_de(&ctx, &mut j, 3200.0);
        assert!(contem(&textos, "Por que este padrão?"), "{textos:?}");
        assert!(!contem(&textos, "Venceu:"), "{textos:?}");
        j.reconhecimento.cartoes[0].por_que_aberto = true;
        j.reconhecimento.cartoes[1].por_que_aberto = true;
        let _ = textos_numa_tela_de(&ctx, &mut j, 3200.0);
        let textos = textos_numa_tela_de(&ctx, &mut j, 3200.0);
        assert!(
            contem(&textos, "Entrada: documento (application/pdf)"),
            "{textos:?}"
        );
        assert!(
            contem(
                &textos,
                "Venceu: Comprovante Pix · Banco Exemplo · versão 2."
            ),
            "{textos:?}"
        );
        assert!(
            contem(&textos, "foi o único padrão que casou"),
            "{textos:?}"
        );
        // O ambíguo: os dois padrões, o campo em que discordam e o que cada um lê.
        assert!(
            contem(&textos, "Ambíguo: 2 padrões empataram"),
            "{textos:?}"
        );
        assert!(
            contem(&textos, "discordam em o tipo documental"),
            "{textos:?}"
        );
        assert!(contem(&textos, "lê comprovante-pix"), "{textos:?}");
        assert!(contem(&textos, "lê pix-agendado"), "{textos:?}");
        assert!(
            contem(&textos, "prioridade 0 · especificidade 2"),
            "{textos:?}"
        );
    }

    #[test]
    fn os_pesos_do_cartao_vem_do_catalogo() {
        let (mut j, _) = janela();
        let _a = soltar(&mut j, NOME_DO_PIX);
        let c = &j.reconhecimento.cartoes[0];
        let peso = c.pesos_das_regras.get(&(PIX.to_owned(), 2)).expect("peso");
        assert_eq!((peso.prioridade, peso.especificidade), (0, 2));
    }

    #[test]
    fn o_documento_de_referencia_e_o_do_cartao_mais_recente_com_a_entrada() {
        let (mut j, _) = janela();
        assert!(j.documento_de_referencia_dos_padroes().is_none());
        let _a = soltar(&mut j, NOME_DO_PIX);
        let _b = soltar(&mut j, NOME_DO_AMBIGUO);
        let d = j.documento_de_referencia_dos_padroes().expect("documento");
        assert_eq!(d.rotulo, NOME_DO_AMBIGUO);
        assert!(d.entrada.texto.is_some());
        // O filtro "casam agora" da área Padrões confere contra ele.
        j.regras.definir_documento(Some(d));
        j.regras
            .definir_filtro(crate::padroes_visiveis::FiltroDePadroes {
                casam: crate::padroes_visiveis::CasamComODocumento::Agora,
                ..Default::default()
            });
        let ids: Vec<String> = j
            .regras
            .padroes_filtrados()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(
            ids,
            [
                "comprovante-pix-banco-exemplo",
                "pix-agendado-banco-exemplo"
            ]
        );
    }
}
