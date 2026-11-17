//! O reconhecedor da demonstração, dos testes e das capturas: um contracheque inventado
//! (o mesmo de `sbf-planilha/tests`) com sugestões montadas contra o Salário do
//! planejamento fictício. Nada aqui lê arquivo, cofre ou rede.

use std::sync::Arc;

use sbf_planejamento::{Centavos, Conciliacao, Data, IdOcorrencia, TipoDeDocumento};
use sbf_planilha::contracheque;
use sbf_reconhecimento::descontos::propor_descontos;
use sbf_reconhecimento::{
    ErroDoReconhecimento, PROVAS_POSSIVEIS, ReconhecedorFicticio, Reconhecido, Resultado, Sugestao,
    UsoDeRubricas, ocorrencia_do_mes,
};

use crate::controle::competencia_texto;
use crate::janela::Janela;

/// O recorrente do Salário no planejamento fictício.
pub const ID_DO_SALARIO: &str = "rec_salario";

/// O movimento fictício do crédito do Salário no extrato.
pub const MOVIMENTO_DO_SALARIO: &str = "pluggy:txn_salario_ficticio";

/// O valor digitado do Salário no planejamento fictício (para a divergência).
const VALOR_DIGITADO_DO_SALARIO: i64 = 845_000;

const TEXTO_DO_CONTRACHEQUE: &str =
    include_str!("../../sbf-planilha/tests/contracheque-2026-ficticio.txt");

/// O nome do arquivo que o reconhecedor fictício "reconhece", seja qual for o que se solte.
pub const NOME_DO_CONTRACHEQUE: &str = "contracheque-2026-10.pdf";

/// Um [`ReconhecedorFicticio`] que sempre devolve o contracheque fictício de outubro de
/// 2026 (no layout de 2026), com três sugestões: vincular à ocorrência do mês, usar as
/// rubricas (fixas e eventuais) e conciliar com o crédito do extrato.
pub fn reconhecedor_do_contracheque() -> ReconhecedorFicticio {
    let resposta = match contracheque::ler(TEXTO_DO_CONTRACHEQUE) {
        Ok(c) => {
            let sha = crate::demonstracao::hash_do_nome(NOME_DO_CONTRACHEQUE);
            let mes = competencia_texto(c.pagamento);
            let mut avisos = Vec::new();
            if !c.conferido {
                avisos
                    .push("A soma das rubricas não bate com o líquido do contracheque.".to_owned());
            }
            avisos.extend(c.avisos.iter().cloned());
            if c.nao_classificadas > 0 {
                avisos.push(format!(
                    "{} rubricas ficaram sem classificação.",
                    c.nao_classificadas
                ));
            }
            let uso = UsoDeRubricas {
                recorrente: ID_DO_SALARIO.to_owned(),
                nome: "Salário".to_owned(),
                documento: sha.clone(),
                digitado_no_mes: Some(Centavos(VALOR_DIGITADO_DO_SALARIO)),
                digitado_depois: Some(Centavos(VALOR_DIGITADO_DO_SALARIO)),
                contracheque: c.clone(),
            };
            // Os compromissos de desconto em folha que o planejamento fictício ainda não tem.
            let planejamento = crate::planejamento_ficticio::PlanejamentoFicticio::novo(
                Data::de_iso(crate::planejamento_ficticio::HOJE_DA_DEMONSTRACAO).unwrap_or(Data {
                    ano: 2026,
                    mes: 11,
                    dia: 16,
                }),
            );
            let descontos = propor_descontos(&planejamento, &uso).ok().flatten();
            let mut sugestoes = vec![
                Sugestao::Vincular {
                    alvo: ocorrencia_do_mes(ID_DO_SALARIO, c.pagamento),
                    rotulo: format!("Vincular ao Salário · {mes}"),
                    tipo: TipoDeDocumento::Contracheque,
                    data: None,
                },
                Sugestao::UsarRubricas {
                    rotulo: uso.rotulo(&uso.eventuais_iniciais()),
                    uso: Box::new(uso),
                },
            ];
            sugestoes.extend(descontos);
            // O crédito do extrato fictício caiu três dias antes da data impressa no
            // contracheque (a data impressa não é a do crédito).
            if let (Some(impressa), Some(liquido)) = (c.credito, c.liquido) {
                let credito = impressa.somar_dias(-3);
                sugestoes.push(Sugestao::Conciliar {
                    rotulo: format!(
                        "Conciliar o Salário · {mes} com o crédito de {:02}/{:02} no extrato",
                        credito.dia, credito.mes
                    ),
                    conciliacao: Conciliacao {
                        ocorrencia: IdOcorrencia {
                            recorrente: ID_DO_SALARIO.to_owned(),
                            competencia: c.pagamento,
                            parcela: None,
                        },
                        movimento: MOVIMENTO_DO_SALARIO.to_owned(),
                        valor: liquido,
                        data: credito,
                        confirmada: false,
                    },
                    provas: PROVAS_POSSIVEIS,
                });
            }
            Ok(Resultado {
                sha256: sha,
                nome: NOME_DO_CONTRACHEQUE.to_owned(),
                ja_existia: false,
                texto: sbf_reconhecimento::EstadoDoTexto::Extraido,
                reconhecido: Reconhecido::Contracheque {
                    consignacoes: c.consignacoes_com_prazo(),
                    contracheque: c,
                },
                sugestoes,
                avisos,
            })
        }
        Err(_) => Err(ErroDoReconhecimento::Arquivo(
            "o contracheque fictício não abriu".to_owned(),
        )),
    };
    ReconhecedorFicticio::novo(resposta)
}

/// Liga o reconhecedor fictício na janela da demonstração.
pub fn ligar_na_demonstracao(janela: &mut Janela) {
    janela.ligar_reconhecedor(Arc::new(reconhecedor_do_contracheque()));
}
