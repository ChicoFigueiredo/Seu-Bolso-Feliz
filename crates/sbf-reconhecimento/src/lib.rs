//! Reconhecimento de um arquivo solto na janela (spec 11, "Arrastar documento").
//!
//! O usuário arrasta um arquivo (o contracheque de outubro, um boleto) para a janela. O
//! reconhecimento roda numa thread, etapa por etapa:
//!
//! 1. [`Etapa::Conferir`]: sha256; o inventário já tem o arquivo?
//! 2. [`Etapa::Registrar`]: vira achado (fonte `arrastado`) com cópia no acervo do cofre. O
//!    original **fica onde está**: nada fora do cofre é apagado nem movido.
//! 3. [`Etapa::ExtrairTexto`]: o texto vai para a tabela `texto` do inventário (PDF, inclusive
//!    protegido, decifrado em memória pelas regras do chaveiro).
//! 4. [`Etapa::LerLocalmente`]: os leitores locais (contracheque; depois boleto, fatura e
//!    comprovante).
//! 5. [`Etapa::PerguntarAIa`]: **só quando os leitores locais não reconhecem**, a triagem por
//!    IA, no nível de confiança do destinatário (ADR-015). A resposta é uma sugestão da
//!    triagem, nunca uma decisão.
//! 6. [`Etapa::Sugerir`]: o que ligar no planejamento.
//!
//! **Nada muda no planejamento sem o clique do usuário.** O reconhecimento escreve no
//! inventário (achado, acervo, texto, sugestão da IA), como o worker faria com o mesmo
//! arquivo numa pasta vigiada; as [`Sugestao`]es só viram [`Alteracao`] quando a janela as
//! confirma.
//!
//! Costura entre dois agentes (2026-11-16): o agente do motor implementa o reconhecedor real
//! (módulos novos neste crate); o agente da janela escreve contra [`Reconhecedor`] e
//! [`ReconhecedorFicticio`]. Mudar um tipo público exige avisar o integrador.

pub mod cofre;
pub mod salario;

pub use cofre::{MotorDeIa, ReconhecedorDoCofre, Segredos};

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use sbf_planejamento::{
    Alteracao, AlvoDoDocumento, Centavos, Competencia, Composicao, Conciliacao, Data, IdOcorrencia,
    TipoDeDocumento,
};
use sbf_planilha::contracheque::{ConsignacaoComPrazo, Contracheque};

/// As etapas, na ordem em que rodam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Etapa {
    Conferir,
    Registrar,
    ExtrairTexto,
    LerLocalmente,
    PerguntarAIa,
    Sugerir,
}

impl Etapa {
    pub const TODAS: [Etapa; 6] = [
        Etapa::Conferir,
        Etapa::Registrar,
        Etapa::ExtrairTexto,
        Etapa::LerLocalmente,
        Etapa::PerguntarAIa,
        Etapa::Sugerir,
    ];

    /// O rótulo da etapa na janela.
    pub fn rotulo(self) -> &'static str {
        match self {
            Etapa::Conferir => "Conferir no inventário",
            Etapa::Registrar => "Guardar no cofre",
            Etapa::ExtrairTexto => "Extrair o texto",
            Etapa::LerLocalmente => "Leitores locais",
            Etapa::PerguntarAIa => "Perguntar à IA",
            Etapa::Sugerir => "Sugerir ligações",
        }
    }
}

/// Como uma etapa está.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Situacao {
    Rodando,
    Feita,
    /// Não precisou rodar (o motivo, em português: "o inventário já tinha o texto").
    Pulada(String),
    /// Falhou sem derrubar o resto (o motivo, sem conteúdo do documento).
    Falhou(String),
}

/// Um aviso de andamento, mandado pela thread do reconhecimento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Andamento {
    pub etapa: Etapa,
    pub situacao: Situacao,
}

/// O texto do arquivo no inventário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoDoTexto {
    /// Extraído agora e gravado na tabela `texto`.
    Extraido,
    /// O inventário já tinha.
    JaHavia,
    /// Sem texto (imagem, PDF escaneado: candidato a OCR) ou formato sem leitor.
    SemTexto(String),
    /// PDF protegido que nenhuma regra do chaveiro abriu (vira a pendência de sempre).
    Protegido(String),
}

/// O que foi reconhecido no arquivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reconhecido {
    /// Um "DEMONSTRATIVO DE PAGAMENTO", lido pelo leitor local.
    Contracheque {
        contracheque: Contracheque,
        consignacoes: Vec<ConsignacaoComPrazo>,
    },
    /// Os leitores locais não reconheceram e a triagem por IA sugeriu. A sugestão já está
    /// gravada em `sugestao_ia`; aceitar é pela área Triagem (decisão do usuário no log).
    PelaIa {
        /// O id da linha em `sugestao_ia`.
        id_da_sugestao: i64,
        /// Uma linha para o cartão ("Boleto · Fornecedor X · vence 24/11/2026").
        resumo: String,
    },
    /// Nada reconheceu (o motivo, em português).
    Nada { motivo: String },
}

/// Uma ligação que o reconhecimento sugere. Só vira [`Alteracao`] no clique do usuário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sugestao {
    /// Vincular o arquivo a um compromisso inteiro ou a uma ocorrência (o contracheque de
    /// outubro vai para "Salário · 10/2026").
    Vincular {
        alvo: AlvoDoDocumento,
        /// "Vincular ao Salário · 10/2026".
        rotulo: String,
        tipo: TipoDeDocumento,
        data: Option<Data>,
    },
    /// Passar a usar as rubricas do contracheque: com eventuais, duas composições (a do mês e
    /// a das fixas); sem, uma só. A pessoa troca a marcação de cada rubrica no cartão antes
    /// de confirmar, e a janela aplica o que [`UsoDeRubricas::composicoes`] devolve para a
    /// marcação do momento.
    UsarRubricas {
        /// "Usar estas 21 rubricas no Salário desde 10/2026" (para a marcação inicial).
        rotulo: String,
        uso: Box<UsoDeRubricas>,
    },
    /// Conciliar o Salário do mês com o crédito que o extrato tem. Substitui o antigo "informar
    /// recebido": a data impressa no contracheque não é a do crédito, então só se sugere
    /// quando o extrato tem o crédito (e nada quando não tem).
    Conciliar {
        /// "Conciliar o Salário · 10/2026 com o crédito de 06/10 no extrato".
        rotulo: String,
        /// O crédito do extrato. A janela aplica com `confirmada: true` (o clique do usuário
        /// é a confirmação).
        conciliacao: Conciliacao,
        /// Quantas das três provas batem: o documento (o contracheque do mês), o valor exato
        /// do líquido e a conta ou a assinatura. De 2 a [`PROVAS_POSSIVEIS`].
        provas: u8,
    },
}

/// O total de provas que uma conciliação do contracheque pode ter.
pub const PROVAS_POSSIVEIS: u8 = 3;

/// O que a sugestão "usar estas rubricas" precisa para montar as composições na hora de
/// confirmar, com a marcação de fixas e eventuais que a pessoa deixou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsoDeRubricas {
    /// O recorrente do salário.
    pub recorrente: String,
    /// O nome do recorrente ("Salário"), para os rótulos.
    pub nome: String,
    /// O sha256 do contracheque (vira o `documento` das composições).
    pub documento: String,
    /// O contracheque lido: rubricas, marcação inicial, informativas e competência.
    pub contracheque: Contracheque,
    /// O valor digitado do recorrente no mês do pagamento.
    pub digitado_no_mes: Option<Centavos>,
    /// O valor digitado do recorrente no mês seguinte (o que vale daqui para frente).
    pub digitado_depois: Option<Centavos>,
}

impl UsoDeRubricas {
    /// A marcação inicial: `true` = a rubrica é eventual (só deste mês).
    #[must_use]
    pub fn eventuais_iniciais(&self) -> Vec<bool> {
        self.contracheque.eventuais.clone()
    }

    /// As composições para a marcação dada, na ordem em que se aplicam (a do mês primeiro).
    #[must_use]
    pub fn composicoes(&self, eventuais: &[bool]) -> Vec<Composicao> {
        self.contracheque
            .composicoes(&self.recorrente, &self.documento, eventuais)
    }

    /// O líquido do mês (sem as rubricas informativas).
    #[must_use]
    pub fn liquido_do_mes(&self) -> Centavos {
        self.contracheque.liquido_do_mes()
    }

    /// O líquido das fixas.
    #[must_use]
    pub fn liquido_das_fixas(&self, eventuais: &[bool]) -> Centavos {
        self.contracheque.liquido_das_fixas(eventuais)
    }

    /// Desde quando as fixas valem.
    #[must_use]
    pub fn fixas_desde(&self, eventuais: &[bool]) -> Competencia {
        self.contracheque.fixas_desde(eventuais)
    }

    /// Há rubrica eventual na marcação (então são duas composições).
    #[must_use]
    pub fn tem_eventuais(&self, eventuais: &[bool]) -> bool {
        self.contracheque.tem_eventuais(eventuais)
    }

    /// O valor digitado com que o líquido das fixas se compara: o do mês seguinte quando as
    /// fixas começam lá, senão o do mês do pagamento.
    #[must_use]
    pub fn digitado_das_fixas(&self, eventuais: &[bool]) -> Option<Centavos> {
        if self.tem_eventuais(eventuais) {
            self.digitado_depois
        } else {
            self.digitado_no_mes
        }
    }

    /// Quanto o líquido das fixas difere do valor digitado (o que vale daqui para frente);
    /// `None` quando bate ou não há valor digitado.
    #[must_use]
    pub fn divergencia(&self, eventuais: &[bool]) -> Option<Centavos> {
        let d = self.digitado_das_fixas(eventuais)?;
        let dif = Centavos(self.liquido_das_fixas(eventuais).0 - d.0);
        (dif != Centavos(0)).then_some(dif)
    }

    /// O rótulo para a marcação dada.
    #[must_use]
    pub fn rotulo(&self, eventuais: &[bool]) -> String {
        let c = &self.contracheque;
        let mes = format!("{:02}/{}", c.pagamento.mes, c.pagamento.ano);
        let comps = self.composicoes(eventuais);
        match comps.as_slice() {
            [so] => format!(
                "Usar estas {} rubricas no {} desde {mes}",
                so.rubricas.len(),
                self.nome
            ),
            [do_mes, fixas] => {
                let de = fixas.desde;
                format!(
                    "Usar as {} rubricas do mês no {} e as {} fixas desde {:02}/{}",
                    do_mes.rubricas.len(),
                    self.nome,
                    fixas.rubricas.len(),
                    de.mes,
                    de.ano
                )
            }
            _ => format!("Usar as rubricas no {} desde {mes}", self.nome),
        }
    }
}

impl Sugestao {
    pub fn rotulo(&self) -> &str {
        match self {
            Sugestao::Vincular { rotulo, .. }
            | Sugestao::UsarRubricas { rotulo, .. }
            | Sugestao::Conciliar { rotulo, .. } => rotulo,
        }
    }

    /// O recorrente cuja `versao_do_registro` a janela lê na hora de confirmar (só
    /// [`Sugestao::UsarRubricas`] precisa; para as duas composições a janela lê de novo a
    /// cada uma).
    pub fn recorrente_para_versao(&self) -> Option<&str> {
        match self {
            Sugestao::UsarRubricas { uso, .. } => Some(&uso.recorrente),
            Sugestao::Vincular { .. } | Sugestao::Conciliar { .. } => None,
        }
    }

    /// As alterações que a janela aplica, em ordem, quando o usuário confirma.
    /// `versao_base` é a do recorrente lida na hora (ignorada fora de
    /// [`Sugestao::UsarRubricas`], onde a janela troca a base de cada composição pela
    /// versão lida logo antes de aplicá-la). `eventuais` é a marcação do cartão (`None`: a
    /// inicial); só importa para [`Sugestao::UsarRubricas`].
    pub fn alteracoes(
        &self,
        sha256: &str,
        nome: &str,
        versao_base: i64,
        eventuais: Option<&[bool]>,
    ) -> Vec<Alteracao> {
        match self {
            Sugestao::Vincular {
                alvo, tipo, data, ..
            } => vec![Alteracao::VincularDocumento {
                alvo: alvo.clone(),
                sha256: sha256.to_owned(),
                tipo_documento: *tipo,
                nome: nome.to_owned(),
                data: *data,
            }],
            Sugestao::UsarRubricas { uso, .. } => {
                let marcacao = eventuais.map_or_else(|| uso.eventuais_iniciais(), <[bool]>::to_vec);
                uso.composicoes(&marcacao)
                    .into_iter()
                    .map(|composicao| Alteracao::DefinirComposicao {
                        composicao,
                        versao_base,
                    })
                    .collect()
            }
            Sugestao::Conciliar { conciliacao, .. } => vec![Alteracao::Conciliar {
                conciliacao: Conciliacao {
                    confirmada: true,
                    ..conciliacao.clone()
                },
            }],
        }
    }
}

/// O resultado de um arquivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resultado {
    pub sha256: String,
    /// Só o nome do arquivo, sem a pasta.
    pub nome: String,
    /// O inventário já tinha este arquivo (pelo sha256) antes de soltar.
    pub ja_existia: bool,
    pub texto: EstadoDoTexto,
    pub reconhecido: Reconhecido,
    /// Em ordem de utilidade; vazio quando nada a ligar.
    pub sugestoes: Vec<Sugestao>,
    /// Avisos para o cartão ("a soma das rubricas não bate com o líquido").
    pub avisos: Vec<String>,
}

/// Por que o reconhecimento parou.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDoReconhecimento {
    /// Não deu para ler o arquivo (sumiu, sem permissão, é pasta).
    #[error("não deu para ler o arquivo: {0}")]
    Arquivo(String),
    /// Falha do inventário ou do planejamento (banco ocupado, migração).
    #[error("falha do banco: {0}")]
    Banco(String),
    #[error("cancelado")]
    Cancelado,
}

/// Quem reconhece. Roda **fora** da thread da interface: cada chamada abre as próprias
/// conexões e pode levar segundos (pdftotext, IA).
pub trait Reconhecedor: Send + Sync + std::fmt::Debug {
    /// Reconhece um arquivo. `andamento` recebe cada mudança de etapa, na ordem de
    /// [`Etapa::TODAS`]; `cancelar` é conferido entre as etapas.
    ///
    /// # Errors
    /// [`ErroDoReconhecimento`]; uma etapa que falha sem impedir as outras vira
    /// [`Situacao::Falhou`] e não erro.
    fn reconhecer(
        &self,
        caminho: &Path,
        andamento: &mut dyn FnMut(Andamento),
        cancelar: &AtomicBool,
    ) -> Result<Resultado, ErroDoReconhecimento>;
}

/// Um reconhecedor de mentira, para os testes da janela, as capturas e a demonstração:
/// devolve a resposta dada e anuncia as etapas como feitas (a da IA como pulada).
#[derive(Debug)]
pub struct ReconhecedorFicticio {
    resposta: Result<Resultado, ErroDoReconhecimento>,
    /// Os caminhos recebidos, em ordem (para os testes conferirem).
    pub recebidos: Mutex<Vec<std::path::PathBuf>>,
}

impl ReconhecedorFicticio {
    pub fn novo(resposta: Result<Resultado, ErroDoReconhecimento>) -> Self {
        Self {
            resposta,
            recebidos: Mutex::new(Vec::new()),
        }
    }
}

impl Reconhecedor for ReconhecedorFicticio {
    fn reconhecer(
        &self,
        caminho: &Path,
        andamento: &mut dyn FnMut(Andamento),
        _cancelar: &AtomicBool,
    ) -> Result<Resultado, ErroDoReconhecimento> {
        if let Ok(mut r) = self.recebidos.lock() {
            r.push(caminho.to_path_buf());
        }
        for etapa in Etapa::TODAS {
            andamento(Andamento {
                etapa,
                situacao: Situacao::Rodando,
            });
            let situacao = if etapa == Etapa::PerguntarAIa {
                Situacao::Pulada("os leitores locais reconheceram".to_owned())
            } else {
                Situacao::Feita
            };
            andamento(Andamento { etapa, situacao });
        }
        self.resposta.clone()
    }
}

/// O alvo "ocorrência do mês" de um recorrente (atalho para quem monta sugestões).
pub fn ocorrencia_do_mes(
    recorrente: &str,
    competencia: sbf_planejamento::Competencia,
) -> AlvoDoDocumento {
    AlvoDoDocumento::Ocorrencia {
        id: IdOcorrencia {
            recorrente: recorrente.to_owned(),
            competencia,
            parcela: None,
        },
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_planejamento::Competencia;

    fn resultado() -> Resultado {
        Resultado {
            sha256: "ab".repeat(32),
            nome: "contracheque.pdf".to_owned(),
            ja_existia: false,
            texto: EstadoDoTexto::Extraido,
            reconhecido: Reconhecido::Nada {
                motivo: "teste".to_owned(),
            },
            sugestoes: vec![Sugestao::Vincular {
                alvo: ocorrencia_do_mes("salario", Competencia { ano: 2026, mes: 10 }),
                rotulo: "Vincular ao Salário · 10/2026".to_owned(),
                tipo: TipoDeDocumento::Contracheque,
                data: None,
            }],
            avisos: Vec::new(),
        }
    }

    #[test]
    fn ficticio_anuncia_as_etapas_em_ordem_e_devolve_a_resposta() {
        let r = ReconhecedorFicticio::novo(Ok(resultado()));
        let mut vistos = Vec::new();
        let saida = r
            .reconhecer(
                Path::new("/x/a.pdf"),
                &mut |a| vistos.push(a),
                &AtomicBool::new(false),
            )
            .expect("resposta");
        assert_eq!(saida, resultado());
        let feitas: Vec<Etapa> = vistos
            .iter()
            .filter(|a| a.situacao != Situacao::Rodando)
            .map(|a| a.etapa)
            .collect();
        assert_eq!(feitas, Etapa::TODAS.to_vec());
    }

    #[test]
    fn vincular_vira_alteracao_com_o_sha_e_o_nome() {
        let s = &resultado().sugestoes[0];
        assert_eq!(s.recorrente_para_versao(), None);
        let alteracoes = s.alteracoes("ff", "c.pdf", 7, None);
        assert_eq!(alteracoes.len(), 1);
        match &alteracoes[0] {
            Alteracao::VincularDocumento {
                sha256,
                nome,
                tipo_documento,
                ..
            } => {
                assert_eq!(sha256, "ff");
                assert_eq!(nome, "c.pdf");
                assert_eq!(*tipo_documento, TipoDeDocumento::Contracheque);
            }
            outra => panic!("alteração inesperada: {outra:?}"),
        }
    }
}
