//! O reconhecedor da demonstração, dos testes e das capturas: um contracheque inventado
//! (o mesmo de `sbf-planilha/tests`) com sugestões montadas contra o Salário do
//! planejamento fictício. Nada aqui lê arquivo, cofre ou rede.

use std::sync::Arc;

use sbf_planejamento::{Centavos, TipoDeDocumento};
use sbf_planilha::contracheque;
use sbf_reconhecimento::{
    ErroDoReconhecimento, ReconhecedorFicticio, Reconhecido, Resultado, Sugestao, ocorrencia_do_mes,
};

use crate::controle::competencia_texto;
use crate::janela::Janela;

/// O recorrente do Salário no planejamento fictício.
pub const ID_DO_SALARIO: &str = "rec_salario";

/// O valor digitado do Salário no planejamento fictício (para a divergência).
const VALOR_DIGITADO_DO_SALARIO: i64 = 845_000;

const TEXTO_DO_CONTRACHEQUE: &str =
    include_str!("../../sbf-planilha/tests/contracheque-ficticio.txt");

/// O nome do arquivo que o reconhecedor fictício "reconhece", seja qual for o que se solte.
pub const NOME_DO_CONTRACHEQUE: &str = "contracheque-2026-10.pdf";

/// Um [`ReconhecedorFicticio`] que sempre devolve o contracheque fictício de outubro de
/// 2026, com duas sugestões: vincular à ocorrência do mês e usar as rubricas.
pub fn reconhecedor_do_contracheque() -> ReconhecedorFicticio {
    let resposta = match contracheque::ler(TEXTO_DO_CONTRACHEQUE) {
        Ok(c) => {
            let sha = crate::demonstracao::hash_do_nome(NOME_DO_CONTRACHEQUE);
            let composicao = c.composicao(ID_DO_SALARIO, &sha);
            let liquido = composicao.liquido();
            let divergencia = (liquido.0 != VALOR_DIGITADO_DO_SALARIO)
                .then_some(Centavos(liquido.0 - VALOR_DIGITADO_DO_SALARIO));
            let mes = competencia_texto(c.pagamento);
            let mut avisos = Vec::new();
            if !c.conferido {
                avisos
                    .push("A soma das rubricas não bate com o líquido do contracheque.".to_owned());
            }
            if c.nao_classificadas > 0 {
                avisos.push(format!(
                    "{} rubricas ficaram sem classificação.",
                    c.nao_classificadas
                ));
            }
            let sugestoes = vec![
                Sugestao::Vincular {
                    alvo: ocorrencia_do_mes(ID_DO_SALARIO, c.pagamento),
                    rotulo: format!("Vincular ao Salário · {mes}"),
                    tipo: TipoDeDocumento::Contracheque,
                    data: None,
                },
                Sugestao::UsarRubricas {
                    rotulo: format!(
                        "Usar estas {} rubricas no Salário desde {mes}",
                        composicao.rubricas.len()
                    ),
                    composicao,
                    divergencia,
                },
            ];
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
