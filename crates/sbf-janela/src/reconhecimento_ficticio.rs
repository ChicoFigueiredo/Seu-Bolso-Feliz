//! O reconhecedor da demonstração, dos testes e das capturas: um contracheque inventado
//! (o mesmo de `sbf-planilha/tests`) com sugestões montadas contra o Salário do
//! planejamento fictício. Nada aqui lê arquivo, cofre ou rede.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sbf_planejamento::{
    Centavos, Competencia, Conciliacao, Data, IdOcorrencia, MovimentoDoExtrato, TipoDeDocumento,
};
use sbf_planilha::contracheque;
use sbf_reconhecimento::descontos::propor_descontos;
use sbf_reconhecimento::regras::entrada_do_documento;
use sbf_reconhecimento::{
    Andamento, ErroDoReconhecimento, PROVAS_POSSIVEIS, Reconhecedor, ReconhecedorFicticio,
    Reconhecido, Resultado, Sugestao, UsoDeRubricas, ocorrencia_do_mes,
};
use sbf_regras::motor::Motor;
use sbf_regras::{Canal, Decisao};

use crate::regras::{FonteDasRegras, PIX_AGENDADO_TEXTO};

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
                entrada: None,
                leitura: None,
            })
        }
        Err(_) => Err(ErroDoReconhecimento::Arquivo(
            "o contracheque fictício não abriu".to_owned(),
        )),
    };
    ReconhecedorFicticio::novo(resposta)
}

/// O nome (qualquer arquivo com "pix" no nome) que a demonstração reconhece pela regra do
/// Pix, com a sugestão de provar o débito.
pub const NOME_DO_PIX: &str = "comprovante-pix-2026-11.pdf";

/// O nome (com "agendado" no nome) cujo comprovante duas regras leem diferente.
pub const NOME_DO_AMBIGUO: &str = "comprovante-pix-agendado.pdf";

/// Um comprovante de Pix inventado, que a regra do Pix (versão 2) do catálogo fictício lê.
pub const TEXTO_DO_PIX: &str = "Banco Exemplo\nComprovante de Pix\nValor: R$ 187,50\nData: 12/11/2026\nPara: Fulano de Tal\nAutenticação: D7K2-9QX1-EXEMPLO";

/// Lê um texto com o motor das regras vigentes do catálogo fictício, registra a execução e a
/// proposta (como o reconhecedor real faz) e monta o [`Resultado`] do cartão. Dados
/// inventados; nada sai do processo.
fn resultado_pelo_motor(fonte: &FonteDasRegras, nome: &str, texto: &str) -> Option<Resultado> {
    let mut catalogo = fonte.catalogo.lock().ok()?;
    let motor = Motor::das_vigentes(&catalogo, fonte.executor.clone()).ok()?;
    let sha = crate::demonstracao::hash_do_nome(nome);
    let entrada = entrada_do_documento(
        &sha,
        nome,
        "application/pdf",
        Some(texto),
        Canal::Arrastado,
        None,
    );
    let leitura = motor.ler(&entrada);
    catalogo.registrar_execucao(&leitura).ok()?;
    let (reconhecido, sugestoes) = match leitura.decisao.clone() {
        Decisao::Reconhecido(rec) => {
            catalogo.registrar_proposta(&rec, &entrada.id).ok()?;
            let sugestoes = sugestoes_do_pix(&rec);
            (
                Reconhecido::PelaRegra {
                    reconhecimento: rec,
                },
                sugestoes,
            )
        }
        Decisao::Ambiguo { candidatas, motivo } => {
            (Reconhecido::Ambiguo { candidatas, motivo }, Vec::new())
        }
        Decisao::Nenhuma => return None,
    };
    Some(Resultado {
        sha256: sha,
        nome: nome.to_owned(),
        ja_existia: false,
        texto: sbf_reconhecimento::EstadoDoTexto::Extraido,
        reconhecido,
        sugestoes,
        avisos: Vec::new(),
        entrada: Some(entrada),
        leitura: Some(leitura),
    })
}

/// A proposta `ProvarRegistro` da regra do Pix vira a sugestão: o comprovante prova um débito
/// que já está no extrato fictício e liga-se à ocorrência da energia de novembro.
fn sugestoes_do_pix(rec: &sbf_regras::Reconhecimento) -> Vec<Sugestao> {
    if !rec
        .propostas
        .iter()
        .any(|p| matches!(p, sbf_regras::Proposta::ProvarRegistro { .. }))
    {
        return Vec::new();
    }
    let data = Data {
        ano: 2026,
        mes: 11,
        dia: 12,
    };
    vec![Sugestao::ProvarRegistro {
        rotulo: "Este comprovante prova o débito de 12/11 · R$ 187,50".to_owned(),
        movimento: MovimentoDoExtrato {
            movimento: "pluggy:txn_pix_ficticio".to_owned(),
            data,
            valor: Centavos(-18_750),
            conta_do_extrato: "Conta Corrente Exemplo".to_owned(),
            descricao: "PIX ENVIADO FULANO DE TAL".to_owned(),
            tipo: "debito".to_owned(),
            contraparte: None,
            contraparte_doc: None,
        },
        alvo: Some(ocorrencia_do_mes(
            "rec_energia",
            Competencia { ano: 2026, mes: 11 },
        )),
        tipo: TipoDeDocumento::Comprovante,
        regra: (rec.regra.clone(), rec.versao),
        chave: rec.chave.clone(),
        provas: 2,
        automatica: false,
    }]
}

/// O reconhecedor da demonstração: o contracheque de sempre, e, com o catálogo de regras
/// fictício ligado, o comprovante de Pix reconhecido pela regra (nome com "pix") e o caso
/// ambíguo (nome com "agendado").
#[derive(Debug)]
pub struct ReconhecedorDeDemonstracao {
    contracheque: ReconhecedorFicticio,
    pix: Option<ReconhecedorFicticio>,
    ambiguo: Option<ReconhecedorFicticio>,
}

impl Reconhecedor for ReconhecedorDeDemonstracao {
    fn reconhecer(
        &self,
        caminho: &Path,
        andamento: &mut dyn FnMut(Andamento),
        cancelar: &AtomicBool,
    ) -> Result<Resultado, ErroDoReconhecimento> {
        let nome = caminho
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let escolhido = if nome.contains("agendado") {
            self.ambiguo.as_ref()
        } else if nome.contains("pix") {
            self.pix.as_ref()
        } else {
            None
        };
        escolhido
            .unwrap_or(&self.contracheque)
            .reconhecer(caminho, andamento, cancelar)
    }
}

/// O reconhecedor da demonstração, com o catálogo fictício de regras quando há.
pub fn reconhecedor_de_demonstracao(fonte: Option<&FonteDasRegras>) -> ReconhecedorDeDemonstracao {
    let pelo_motor = |nome: &str, texto: &str| {
        fonte
            .and_then(|f| resultado_pelo_motor(f, nome, texto))
            .map(|r| ReconhecedorFicticio::novo(Ok(r)))
    };
    ReconhecedorDeDemonstracao {
        contracheque: reconhecedor_do_contracheque(),
        pix: pelo_motor(NOME_DO_PIX, TEXTO_DO_PIX),
        ambiguo: pelo_motor(NOME_DO_AMBIGUO, PIX_AGENDADO_TEXTO),
    }
}

/// Liga o reconhecedor fictício na janela da demonstração. Ligue o catálogo de regras
/// (`janela.regras.ligar`) antes, para a demonstração ter o Pix e o caso ambíguo.
pub fn ligar_na_demonstracao(janela: &mut Janela) {
    let fonte = janela.regras.fonte().cloned();
    let reconhecedor = Arc::new(reconhecedor_de_demonstracao(fonte.as_ref()));
    janela.ligar_fila(Arc::new(fila_de_demonstracao(&*reconhecedor)));
    janela.ligar_reconhecedor(reconhecedor);
}

/// O resultado fictício de um nome de arquivo (o que o reconhecedor da demonstração devolve).
fn resultado_de(r: &dyn Reconhecedor, nome: &str) -> Option<Resultado> {
    r.reconhecer(Path::new(nome), &mut |_| {}, &AtomicBool::new(false))
        .ok()
}

/// O que a demonstração diz que não tem padrão: uma foto de recibo, lida por OCR.
pub const NOME_DO_RECIBO_SEM_PADRAO: &str = "foto-recibo-oficina.jpg";

/// A Caixa de entrada fictícia da demonstração: o contracheque (Telegram), o comprovante de
/// Pix (Gmail), o caso ambíguo (pasta) e uma foto sem padrão (celular).
pub fn fila_de_demonstracao(r: &dyn Reconhecedor) -> crate::caixa_de_entrada::FilaFicticia {
    use crate::caixa_de_entrada::{FilaFicticia, entrada_ficticia};
    use sbf_inventario::caixa_de_entrada::{Estado, Origem};
    let mut fila = FilaFicticia::nova();
    let linhas = [
        (
            NOME_DO_CONTRACHEQUE,
            "2026-11-20T12:10:00.000Z",
            Origem::Telegram,
            Estado::Pronto,
        ),
        (
            NOME_DO_PIX,
            "2026-11-20T09:42:00.000Z",
            Origem::Gmail,
            Estado::Pronto,
        ),
        (
            NOME_DO_AMBIGUO,
            "2026-11-19T21:03:00.000Z",
            Origem::Pasta,
            Estado::PrecisaRevisar,
        ),
    ];
    for (nome, chegou, origem, estado) in linhas {
        // Sem o catálogo de regras o Pix e o ambíguo caem no contracheque: não entram.
        let res = resultado_de(r, nome)
            .filter(|res| res.sha256 == crate::demonstracao::hash_do_nome(nome));
        if let Some(res) = res {
            let (tipo, resumo) = sbf_reconhecimento::fila::tipo_e_resumo(&res);
            let e = entrada_ficticia(
                &res.sha256,
                nome,
                chegou,
                origem,
                tipo.as_deref(),
                &resumo.unwrap_or_default(),
                estado,
            );
            fila = fila.com(e, res);
        }
    }
    let sha = crate::demonstracao::hash_do_nome(NOME_DO_RECIBO_SEM_PADRAO);
    let entrada = entrada_do_documento(
        &sha,
        NOME_DO_RECIBO_SEM_PADRAO,
        "image/jpeg",
        Some("RECIBO OFICINA EXEMPLO servico 12/11/2026 R$ 320,00"),
        Canal::Arrastado,
        None,
    );
    let res = Resultado {
        sha256: sha.clone(),
        nome: NOME_DO_RECIBO_SEM_PADRAO.to_owned(),
        ja_existia: false,
        texto: sbf_reconhecimento::EstadoDoTexto::Extraido,
        reconhecido: Reconhecido::Nada {
            motivo: "nenhum leitor local reconheceu o documento".to_owned(),
        },
        sugestoes: Vec::new(),
        avisos: Vec::new(),
        entrada: Some(entrada),
        leitura: None,
    };
    let e = entrada_ficticia(
        &sha,
        NOME_DO_RECIBO_SEM_PADRAO,
        "2026-11-20T10:30:00.000Z",
        Origem::Celular,
        None,
        "nenhum leitor local reconheceu o documento",
        Estado::NaoReconhecido,
    );
    fila.com(e, res)
}
