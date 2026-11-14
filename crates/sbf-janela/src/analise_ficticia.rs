//! Perguntar e Pontos de atenção da **demonstração**: dados inventados, sobre o motor
//! verdadeiro (índice em memória com o embedder falso, `perguntar` de verdade com o provedor
//! falso, o `Armazem` de pontos em memória). Nenhum modelo é baixado e nenhuma IA é chamada.

use std::sync::Arc;

use sbf_ia::{Consumo, Falso, NivelDeConfianca, Resposta as RespostaDoMotor};
use sbf_indice::perguntar::{K_PADRAO, RegistroEmMemoria, Resposta, ServicoDePerguntar, perguntar};
use sbf_indice::{BuscaEmMemoria, Embedder, EmbedderFalso, Indice, ItemRef, Trecho};
use sbf_inventario::Ref;
use sbf_pontos::{
    Area, Armazem, Candidato, Evidencia, Impacto, PontosLocais, ServicoDePontos, Unidade,
};

const TEXTOS: [&str; 6] = [
    "Fatura de energia elétrica da Energia Exemplo, referente a outubro de 2026. Total R$ 210,00, \
     vencimento em 10/11/2026. Titular CPF 123.456.789-09.",
    "Fatura de energia elétrica da Energia Exemplo, referente a setembro de 2026. Total R$ 198,40, \
     pago em 10/10/2026.",
    "Mensalidade da Academia Corpo Exemplo, R$ 119,90, cobrança recorrente no cartão final 1234.",
    "Boleto do condomínio Edifício Fictício, R$ 685,00, vencimento 05/11/2026.",
    "Apólice do seguro do carro, Seguradora Exemplo, parcela 7 de 12 de R$ 289,00.",
    "Comprovante de PIX para Padaria Pão Quente, R$ 23,40, em 03/11/2026.",
];

/// Perguntar da demonstração, sobre os `itens` (os links abrem itens da demonstração).
#[must_use]
pub fn perguntar_da_demonstracao(itens: &[Ref]) -> Box<dyn ServicoDePerguntar> {
    let embedder: Arc<dyn Embedder> = Arc::new(EmbedderFalso::novo());
    let mut indice = Indice::em_memoria().expect("índice em memória");
    let trechos: Vec<Trecho> = itens
        .iter()
        .zip(TEXTOS)
        .map(|(r, t)| Trecho {
            id: format!("achado:{}#0", r.chave),
            item: ItemRef::novo("achado", r.chave.clone()),
            texto: t.to_owned(),
        })
        .collect();
    indice
        .sincronizar("achado", &trechos, embedder.as_ref())
        .expect("sincronizar");
    Box::new(Ficticio {
        busca: BuscaEmMemoria::nova(indice, embedder),
        provedor: Falso::com_funcao(|pedido| {
            // Responde citando os dois primeiros trechos que vieram (a "IA" da demonstração).
            let primeiros: Vec<&str> = pedido
                .entrada
                .lines()
                .filter(|l| l.starts_with("[1]") || l.starts_with("[2]"))
                .collect();
            let resumo = primeiros
                .first()
                .map_or("nada", |l| l.trim_start_matches("[1] "));
            Ok(RespostaDoMotor {
                json: serde_json::json!({
                    "resposta": format!(
                        "Pelo que está nos seus documentos: {} (resposta da demonstração, sem IA de verdade.)",
                        resumo.chars().take(160).collect::<String>()
                    ),
                    "citacoes": [1, 2]
                }),
                consumo: Consumo {
                    tokens_entrada: 420,
                    tokens_saida: 60,
                    custo_dezmilesimos_usd: 0,
                    nominal: true,
                },
                modelo: "demonstracao".to_owned(),
            })
        }),
        registro: RegistroEmMemoria::novo(),
    })
}

struct Ficticio {
    busca: BuscaEmMemoria,
    provedor: Falso,
    registro: RegistroEmMemoria,
}

impl ServicoDePerguntar for Ficticio {
    fn perguntar(&mut self, pergunta: &str) -> Result<Resposta, String> {
        perguntar(
            &self.busca,
            &self.provedor,
            &self.registro,
            NivelDeConfianca::Mascarado,
            pergunta,
            K_PADRAO.min(3),
        )
        .map_err(|e| e.to_string())
    }

    fn trechos_no_indice(&self) -> Option<usize> {
        Some(TEXTOS.len())
    }
}

fn candidato(
    regra: &'static str,
    chave: &str,
    area: Area,
    titulo: &str,
    detalhe: &str,
    evidencia: Vec<Evidencia>,
    impacto: Option<Impacto>,
) -> Candidato {
    Candidato {
        regra,
        chave: chave.to_owned(),
        area,
        titulo: titulo.to_owned(),
        detalhe: detalhe.to_owned(),
        evidencia,
        impacto,
        assinatura: chave.to_owned(),
    }
}

/// Os Pontos de atenção da demonstração: seis pontos inventados, um já resolvido.
#[must_use]
pub fn pontos_da_demonstracao(itens: &[Ref]) -> Box<dyn ServicoDePontos> {
    let ev = |i: usize, rotulo: &str| {
        itens.get(i).map_or_else(
            || Evidencia::nova("fonte", "demonstracao", rotulo),
            |r| Evidencia::nova("achado", r.chave.clone(), rotulo),
        )
    };
    let reais = |valor, periodo: &str, estimativa| Impacto {
        valor,
        unidade: Unidade::CentavosBrl,
        periodo: periodo.to_owned(),
        estimativa,
    };
    let candidatos = vec![
        candidato(
            "assinatura-reajuste",
            "reajuste:f-filmes",
            Area::Dinheiro,
            "Filmes Exemplo subiu de preço",
            "Era R$ 39,90 nos dois meses anteriores e agora é R$ 54,90 (+R$ 15,00).",
            vec![
                ev(0, "Filmes Exemplo, R$ 39,90 (05/09)"),
                ev(1, "Filmes Exemplo, R$ 54,90 (05/11)"),
            ],
            Some(reais(18_000, "ano", true)),
        ),
        candidato(
            "cobranca-sem-documento",
            "sem-documento:f-seguro",
            Area::Dinheiro,
            "Seguro Exemplo cobra todo mês e nenhum documento comprova",
            "3 meses seguidos de cerca de R$ 29,90, sem fatura ou comprovante ligado. Vale conferir se você ainda usa.",
            vec![ev(4, "Seguro Exemplo, R$ 29,90 (10/11)")],
            Some(reais(35_880, "ano", true)),
        ),
        candidato(
            "tarifas-no-ciclo",
            "tarifas-no-ciclo:2026-10",
            Area::Dinheiro,
            "Tarifas bancárias somam mais que o normal",
            "2 lançamentos somam R$ 35,80 no ciclo 20/10/2026 a 19/11/2026.",
            vec![ev(5, "Tarifa, R$ 19,90 (25/10)")],
            Some(reais(3_580, "ciclo", false)),
        ),
        candidato(
            "documentos-parados",
            "a-triar",
            Area::Operacao,
            "12 documentos parados em A triar há mais de 30 dias",
            "O mais antigo chegou em 2026-09-01.",
            vec![ev(2, "boleto-condominio.pdf"), ev(3, "apolice.pdf")],
            Some(Impacto {
                valor: 7,
                unidade: Unidade::Minutos,
                periodo: "unico".into(),
                estimativa: true,
            }),
        ),
        candidato(
            "disco-sem-cifra",
            "disco",
            Area::Operacao,
            "O disco do cofre não está cifrado",
            "Quem pegar o computador lê seus documentos. Ative BitLocker, FileVault ou LUKS.",
            vec![Evidencia::nova("disco", "cofre", "Volume do cofre")],
            None,
        ),
        candidato(
            "ia-consumo-acima-do-ritmo",
            "semana",
            Area::CustoDeIa,
            "O consumo de IA da semana está acima do ritmo",
            "120000 tokens em 31 chamadas nos últimos 7 dias, contra uma média de 10000 por semana nas quatro anteriores.",
            vec![Evidencia::nova(
                "saida_ia",
                "semana",
                "Saídas dos últimos 7 dias",
            )],
            Some(Impacto {
                valor: 250,
                unidade: Unidade::CentavosUsd,
                periodo: "semana".into(),
                estimativa: false,
            }),
        ),
        candidato(
            "juros-e-multa",
            "juros-e-multa:2026-09",
            Area::Dinheiro,
            "Você pagou juros ou multa neste ciclo",
            "1 lançamento soma R$ 17,34 no ciclo 20/09/2026 a 19/10/2026.",
            vec![ev(5, "Multa por atraso, R$ 17,34")],
            Some(reais(1_734, "ciclo", false)),
        ),
    ];
    let mut armazem = Armazem::em_memoria().expect("armazém em memória");
    let avaliadas = ["assinatura-reajuste"];
    armazem
        .aplicar(&candidatos, &avaliadas, "2026-11-14T09:00:00Z")
        .expect("aplicar");
    // O último já foi resolvido pelo usuário (aparece no filtro Resolvidos).
    if let Some(p) = armazem
        .listar(sbf_pontos::Filtro::default())
        .ok()
        .and_then(|l| l.into_iter().find(|p| p.regra == "juros-e-multa"))
    {
        let _ = armazem.resolver(
            p.id,
            "Paguei e já liguei para o banco",
            "2026-11-14T10:00:00Z",
        );
    }
    Box::new(PontosLocais::novo(armazem, |_| {
        Ok(sbf_pontos::Resumo::default())
    }))
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_inventario::Tipo;
    use sbf_pontos::{Estado, Filtro};

    fn itens() -> Vec<Ref> {
        (0..6)
            .map(|i| Ref::nova(Tipo::Achado, format!("hash-demo-{i}")))
            .collect()
    }

    #[test]
    fn perguntar_da_demonstracao_acha_a_fatura_certa_e_cita_com_link() {
        let mut s = perguntar_da_demonstracao(&itens());
        let r = s
            .perguntar("quanto foi a fatura de energia elétrica de outubro?")
            .expect("responde");
        assert_eq!(r.trechos_enviados, 3);
        assert!(!r.citacoes.is_empty());
        assert_eq!(r.citacoes[0].item, ItemRef::novo("achado", "hash-demo-0"));
        assert!(r.citacoes[0].trecho.contains("R$ 210,00"));
        assert_eq!(s.trechos_no_indice(), Some(6));
    }

    #[test]
    fn pontos_da_demonstracao_tem_abertos_estimados_e_um_resolvido() {
        let mut s = pontos_da_demonstracao(&itens());
        let abertos = s
            .listar(Filtro {
                area: None,
                estado: Some(Estado::Aberto),
            })
            .expect("abertos");
        assert_eq!(abertos.len(), 6);
        assert!(
            abertos
                .iter()
                .any(|p| p.impacto.as_ref().is_some_and(|i| i.estimativa))
        );
        let resolvidos = s
            .listar(Filtro {
                area: None,
                estado: Some(Estado::Resolvido),
            })
            .expect("resolvidos");
        assert_eq!(resolvidos.len(), 1);
        assert!(s.ignorar(abertos[0].id, "  ").is_err());
        s.ignorar(abertos[0].id, "já conhecido").expect("ignorar");
        assert_eq!(s.abertos_por_area().expect("n").values().sum::<usize>(), 5);
    }
}
