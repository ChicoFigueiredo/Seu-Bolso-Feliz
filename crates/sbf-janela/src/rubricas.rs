//! O que o Controle mensal mostra de rubricas e de documentos vinculados, sem egui (testável):
//! a tabela de proventos e descontos com o aviso de divergência, os rótulos dos tipos de
//! documento, as linhas da aba Documentos e o seletor que anexa um achado do inventário a um
//! compromisso ou a uma ocorrência.

use sbf_inventario::consulta::{
    Area, Celula, Consulta, Filtro as FiltroDoInventario, PedidoDeTabela, Tabela,
};
use sbf_inventario::{Ref, Tipo};
use sbf_planejamento::{
    Alteracao, AlvoDoDocumento, Centavos, Competencia, Composicao, Data, DocumentoVinculado,
    IdOcorrencia, TipoDeDocumento, TipoDeRubrica,
};

use crate::controle::{competencia_texto, reais};
use crate::formato;

// ---------------------------------------------------------------------------------------
// Rubricas
// ---------------------------------------------------------------------------------------

/// O aviso quando as rubricas não somam o valor digitado; `None` quando batem (ou quando não
/// há valor digitado para comparar).
pub fn aviso_de_divergencia(c: &Composicao, digitado: Option<Centavos>) -> Option<String> {
    let d = digitado?;
    c.divergencia(Some(d))?;
    Some(format!(
        "as rubricas somam {}, o digitado é {}",
        reais(c.liquido()),
        reais(d)
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeRubrica {
    pub codigo: String,
    pub nome: String,
    pub competencia: String,
    pub prazo: String,
    pub valor: Centavos,
    /// O compromisso de desconto em folha que a rubrica representa (vira link para a linha).
    pub recorrente: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabelaDeRubricas {
    pub proventos: Vec<LinhaDeRubrica>,
    pub descontos: Vec<LinhaDeRubrica>,
    pub total_proventos: Centavos,
    pub total_descontos: Centavos,
    /// Proventos menos descontos.
    pub liquido: Centavos,
    pub digitado: Option<Centavos>,
    pub aviso: Option<String>,
    /// "contracheque 11/2026", "planilha 2026.10".
    pub origem: String,
    /// O `content_hash` do documento de onde vieram (abre a prévia).
    pub documento: Option<String>,
}

pub fn tabela_de_rubricas(c: &Composicao, digitado: Option<Centavos>) -> TabelaDeRubricas {
    let linha = |r: &sbf_planejamento::Rubrica| LinhaDeRubrica {
        codigo: r.codigo.clone().unwrap_or_else(|| "—".into()),
        nome: r.nome.clone(),
        competencia: r.competencia.map_or_else(|| "—".into(), competencia_texto),
        prazo: r.prazo.map_or_else(|| "—".into(), |p| format!("{p} meses")),
        valor: r.valor,
        recorrente: r.recorrente.clone(),
    };
    let de = |tipo| {
        c.rubricas
            .iter()
            .filter(move |r| r.tipo == tipo)
            .collect::<Vec<_>>()
    };
    let proventos = de(TipoDeRubrica::Provento);
    let descontos = de(TipoDeRubrica::Desconto);
    let soma = |v: &[&sbf_planejamento::Rubrica]| v.iter().map(|r| r.valor).sum::<Centavos>();
    TabelaDeRubricas {
        total_proventos: soma(&proventos),
        total_descontos: soma(&descontos),
        proventos: proventos.iter().map(|r| linha(r)).collect(),
        descontos: descontos.iter().map(|r| linha(r)).collect(),
        liquido: c.liquido(),
        digitado,
        aviso: aviso_de_divergencia(c, digitado),
        origem: c.origem.clone(),
        documento: c.documento.clone(),
    }
}

// ---------------------------------------------------------------------------------------
// Tipos de documento
// ---------------------------------------------------------------------------------------

pub const TIPOS_DE_DOCUMENTO: [TipoDeDocumento; 7] = [
    TipoDeDocumento::Contracheque,
    TipoDeDocumento::Fatura,
    TipoDeDocumento::Boleto,
    TipoDeDocumento::Comprovante,
    TipoDeDocumento::Contrato,
    TipoDeDocumento::NotaFiscal,
    TipoDeDocumento::Outro,
];

pub fn rotulo_do_tipo(t: TipoDeDocumento) -> &'static str {
    match t {
        TipoDeDocumento::Contracheque => "Contracheque",
        TipoDeDocumento::Fatura => "Fatura",
        TipoDeDocumento::Boleto => "Boleto",
        TipoDeDocumento::Comprovante => "Comprovante",
        TipoDeDocumento::Contrato => "Contrato",
        TipoDeDocumento::NotaFiscal => "Nota fiscal",
        TipoDeDocumento::Outro => "Outro",
    }
}

/// O que o tipo quer dizer: fatura e boleto cobram, só o comprovante prova o pagamento.
pub fn nota_do_tipo(t: TipoDeDocumento) -> Option<&'static str> {
    match t {
        TipoDeDocumento::Fatura | TipoDeDocumento::Boleto => {
            Some("Cobrança: não prova que foi pago.")
        }
        TipoDeDocumento::Comprovante => Some("Prova de pagamento."),
        TipoDeDocumento::Contracheque => Some("Traz as rubricas do salário."),
        _ => None,
    }
}

/// O tipo do documento do inventário (`boleto`, `fatura_cartao`, "Nota fiscal"…) como tipo
/// de vínculo. Não reconhecido: `Outro`.
pub fn tipo_sugerido(tipo_do_achado: &str) -> TipoDeDocumento {
    let t = normalizar(tipo_do_achado);
    for (palavra, tipo) in [
        ("contracheque", TipoDeDocumento::Contracheque),
        ("holerite", TipoDeDocumento::Contracheque),
        ("comprovante", TipoDeDocumento::Comprovante),
        ("fatura", TipoDeDocumento::Fatura),
        ("boleto", TipoDeDocumento::Boleto),
        ("contrato", TipoDeDocumento::Contrato),
        ("nota", TipoDeDocumento::NotaFiscal),
    ] {
        if t.contains(palavra) {
            return tipo;
        }
    }
    TipoDeDocumento::Outro
}

fn normalizar(t: &str) -> String {
    t.to_lowercase().replace(['_', '-'], " ")
}

// ---------------------------------------------------------------------------------------
// A aba Documentos
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeDocumento {
    /// O id do vínculo (para desvincular).
    pub id: String,
    pub sha256: String,
    pub tipo: &'static str,
    pub nota: Option<&'static str>,
    pub nome: String,
    pub data: Option<String>,
    /// "Compromisso inteiro" ou "Só 11/2026".
    pub onde: String,
}

pub fn linhas_de_documentos(docs: &[DocumentoVinculado]) -> Vec<LinhaDeDocumento> {
    docs.iter()
        .filter(|d| !d.desvinculado)
        .map(|d| LinhaDeDocumento {
            id: d.id.clone(),
            sha256: d.sha256.clone(),
            tipo: rotulo_do_tipo(d.tipo),
            nota: nota_do_tipo(d.tipo),
            nome: d.nome.clone(),
            data: d.data.map(crate::controle::data_texto),
            onde: match &d.alvo {
                AlvoDoDocumento::Recorrente { .. } => "Compromisso inteiro".into(),
                AlvoDoDocumento::Ocorrencia { id } => {
                    format!("Só {}", competencia_texto(id.competencia))
                }
            },
        })
        .collect()
}

/// O achado do inventário como a `Ref` que abre a prévia (a chave é o `content_hash`).
pub fn ref_do_documento(sha256: &str) -> Ref {
    Ref::nova(Tipo::Achado, sha256)
}

// ---------------------------------------------------------------------------------------
// O seletor "Anexar documento"
// ---------------------------------------------------------------------------------------

/// Um achado do inventário que pode ser anexado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidato {
    pub sha256: String,
    pub nome: String,
    /// O tipo como o inventário diz (`boleto`, "Fatura de cartão"…).
    pub tipo: String,
    pub data: Option<String>,
    pub fornecedor: Option<String>,
}

/// O que a busca usou, para não repetir a mesma consulta a cada quadro.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChaveDaBusca {
    texto: String,
    periodo: bool,
    competencia: Competencia,
}

/// O diálogo que anexa um documento (um achado do inventário) a um compromisso ou ocorrência.
#[derive(Debug, Clone)]
pub struct SeletorDeDocumento {
    pub recorrente: String,
    pub nome_do_compromisso: String,
    /// A ocorrência escolhida na grade, se havia.
    pub ocorrencia: Option<IdOcorrencia>,
    /// O mês da linha: a busca olha dois meses para trás e um para a frente (o documento
    /// costuma vir antes do vencimento).
    pub competencia: Competencia,
    pub texto: String,
    pub tipo: TipoDeDocumento,
    /// O usuário mexeu no tipo; senão ele segue o do achado escolhido.
    pub tipo_escolhido_a_mao: bool,
    pub filtrar_por_tipo: bool,
    pub filtrar_por_periodo: bool,
    /// Vincular ao compromisso inteiro (senão, só à ocorrência).
    pub ao_compromisso_inteiro: bool,
    pub candidatos: Vec<Candidato>,
    pub escolhido: Option<String>,
    pub erro: Option<String>,
    buscado: Option<ChaveDaBusca>,
}

impl SeletorDeDocumento {
    pub fn novo(
        recorrente: String,
        nome: String,
        ocorrencia: Option<IdOcorrencia>,
        competencia: Competencia,
    ) -> Self {
        Self {
            recorrente,
            nome_do_compromisso: nome,
            ao_compromisso_inteiro: ocorrencia.is_none(),
            ocorrencia,
            competencia,
            texto: String::new(),
            tipo: TipoDeDocumento::Fatura,
            tipo_escolhido_a_mao: false,
            filtrar_por_tipo: true,
            filtrar_por_periodo: true,
            candidatos: Vec::new(),
            escolhido: None,
            erro: None,
            buscado: None,
        }
    }

    /// O filtro do inventário: o texto digitado e, se pedido, o mês da linha e os vizinhos
    /// (dois para trás, um para a frente).
    pub fn filtro(&self) -> FiltroDoInventario {
        let texto = self.texto.trim();
        FiltroDoInventario {
            texto: (!texto.is_empty()).then(|| texto.to_owned()),
            ciclos: self.filtrar_por_periodo.then(|| {
                (
                    self.competencia.somar_meses(-2).to_string(),
                    self.competencia.somar_meses(1).to_string(),
                )
            }),
            ..Default::default()
        }
    }

    fn chave(&self) -> ChaveDaBusca {
        ChaveDaBusca {
            texto: self.texto.trim().to_owned(),
            periodo: self.filtrar_por_periodo,
            competencia: self.competencia,
        }
    }

    /// Busca de novo se o texto ou o período mudaram desde a última vez. O filtro por tipo
    /// é local (não precisa de nova consulta).
    pub fn buscar(&mut self, consulta: &dyn Consulta) {
        let chave = self.chave();
        if self.buscado.as_ref() == Some(&chave) {
            return;
        }
        self.buscado = Some(chave);
        self.erro = None;
        let mut achados: Vec<Candidato> = Vec::new();
        for area in [Area::Documentos, Area::ATriar] {
            let pedido = PedidoDeTabela::Area {
                area,
                filtro: self.filtro(),
            };
            match consulta.tabela(&pedido) {
                Ok(t) => {
                    for c in candidatos_da_tabela(&t) {
                        if !achados.iter().any(|a| a.sha256 == c.sha256) {
                            achados.push(c);
                        }
                    }
                }
                Err(e) => {
                    self.erro = Some(e.to_string());
                    break;
                }
            }
        }
        if self
            .escolhido
            .as_ref()
            .is_some_and(|e| !achados.iter().any(|a| a.sha256 == *e))
        {
            self.escolhido = None;
        }
        self.candidatos = achados;
    }

    /// Os candidatos que passam no filtro por tipo.
    pub fn visiveis(&self) -> Vec<&Candidato> {
        self.candidatos
            .iter()
            .filter(|c| {
                !self.filtrar_por_tipo
                    || self.tipo == TipoDeDocumento::Outro
                    || tipo_sugerido(&c.tipo) == self.tipo
            })
            .collect()
    }

    /// Escolhe um candidato; o tipo acompanha o do achado, a menos que o usuário já tenha
    /// escolhido o dele.
    pub fn escolher(&mut self, sha256: &str) {
        let Some(c) = self.candidatos.iter().find(|c| c.sha256 == sha256) else {
            return;
        };
        if !self.tipo_escolhido_a_mao {
            self.tipo = tipo_sugerido(&c.tipo);
        }
        self.escolhido = Some(sha256.to_owned());
    }

    pub fn alvo(&self) -> AlvoDoDocumento {
        match (&self.ocorrencia, self.ao_compromisso_inteiro) {
            (Some(id), false) => AlvoDoDocumento::Ocorrencia { id: id.clone() },
            _ => AlvoDoDocumento::Recorrente {
                id: self.recorrente.clone(),
            },
        }
    }

    /// A alteração que vincula o achado escolhido, ou o que falta.
    pub fn alteracao(&self) -> Result<Alteracao, String> {
        let sha = self
            .escolhido
            .clone()
            .ok_or("Escolha um documento da lista.")?;
        let c = self
            .candidatos
            .iter()
            .find(|c| c.sha256 == sha)
            .ok_or("O documento escolhido não está mais na lista.")?;
        Ok(Alteracao::VincularDocumento {
            alvo: self.alvo(),
            sha256: sha,
            tipo_documento: self.tipo,
            nome: c.nome.clone(),
            data: c.data.as_deref().and_then(Data::de_iso),
        })
    }
}

fn texto_da(c: &Celula) -> String {
    formato::texto_da_celula(c)
}

/// Os achados de uma tabela do inventário. As colunas são achadas pelo nome (a consulta real
/// e a da demonstração trazem os mesmos rótulos, às vezes com outro nome na data).
pub fn candidatos_da_tabela(t: &Tabela) -> Vec<Candidato> {
    let coluna = |nomes: &[&str]| {
        t.colunas
            .iter()
            .position(|c| nomes.iter().any(|n| c.rotulo.eq_ignore_ascii_case(n)))
    };
    let (c_tipo, c_data, c_forn) = (
        coluna(&["Tipo"]),
        coluna(&["Data", "Data-chave"]),
        coluna(&["Fornecedor"]),
    );
    let opcional = |linha: &sbf_inventario::consulta::Linha, c: Option<usize>| {
        c.and_then(|i| linha.celulas.get(i))
            .map(texto_da)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty() && s != "—")
    };
    t.linhas
        .iter()
        .filter_map(|l| {
            let alvo = l.alvo.as_ref()?;
            if !matches!(alvo.tipo, Tipo::Achado | Tipo::Documento) {
                return None;
            }
            Some(Candidato {
                sha256: alvo.chave.clone(),
                nome: l.celulas.first().map(texto_da).unwrap_or_default(),
                tipo: opcional(l, c_tipo).unwrap_or_default(),
                data: opcional(l, c_data)
                    .and_then(|d| crate::janela::data_iso(&d.chars().take(10).collect::<String>())),
                fornecedor: opcional(l, c_forn),
            })
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_planejamento::Rubrica;

    fn rubrica(tipo: TipoDeRubrica, valor: i64, nome: &str) -> Rubrica {
        Rubrica {
            codigo: Some("1".into()),
            nome: nome.into(),
            tipo,
            valor: Centavos(valor),
            competencia: None,
            prazo: None,
            recorrente: None,
        }
    }

    fn composicao(proventos: i64, descontos: i64) -> Composicao {
        Composicao {
            recorrente: "sal".into(),
            desde: Competencia { ano: 2026, mes: 11 },
            rubricas: vec![
                rubrica(TipoDeRubrica::Provento, proventos, "Vencimento"),
                rubrica(TipoDeRubrica::Desconto, descontos, "INSS"),
            ],
            documento: Some("abc".into()),
            origem: "contracheque 11/2026".into(),
        }
    }

    #[test]
    fn tabela_soma_proventos_descontos_e_liquido() {
        let c = composicao(100_000, 25_000);
        let t = tabela_de_rubricas(&c, Some(Centavos(75_000)));
        assert_eq!(t.total_proventos, Centavos(100_000));
        assert_eq!(t.total_descontos, Centavos(25_000));
        assert_eq!(t.liquido, Centavos(75_000));
        assert_eq!(t.aviso, None);
        assert_eq!(t.proventos.len(), 1);
        assert_eq!(t.descontos[0].nome, "INSS");
        assert_eq!(t.documento.as_deref(), Some("abc"));
    }

    #[test]
    fn divergencia_vira_aviso_com_os_dois_valores() {
        let c = composicao(100_000, 25_000);
        let a = aviso_de_divergencia(&c, Some(Centavos(70_000))).expect("aviso");
        assert_eq!(a, "as rubricas somam R$ 750,00, o digitado é R$ 700,00");
        assert_eq!(aviso_de_divergencia(&c, Some(Centavos(75_000))), None);
        assert_eq!(aviso_de_divergencia(&c, None), None);
    }

    #[test]
    fn fatura_e_boleto_nao_sao_comprovante() {
        for t in [TipoDeDocumento::Fatura, TipoDeDocumento::Boleto] {
            assert!(nota_do_tipo(t).unwrap().contains("não prova"));
        }
        assert_eq!(
            nota_do_tipo(TipoDeDocumento::Comprovante),
            Some("Prova de pagamento.")
        );
        assert_ne!(
            rotulo_do_tipo(TipoDeDocumento::Fatura),
            rotulo_do_tipo(TipoDeDocumento::Comprovante)
        );
    }

    #[test]
    fn tipo_do_achado_vira_tipo_de_vinculo() {
        assert_eq!(tipo_sugerido("fatura_cartao"), TipoDeDocumento::Fatura);
        assert_eq!(tipo_sugerido("Boleto"), TipoDeDocumento::Boleto);
        assert_eq!(tipo_sugerido("nota_fiscal"), TipoDeDocumento::NotaFiscal);
        assert_eq!(tipo_sugerido("contracheque"), TipoDeDocumento::Contracheque);
        assert_eq!(tipo_sugerido("foto"), TipoDeDocumento::Outro);
    }

    fn seletor() -> SeletorDeDocumento {
        let mut s = SeletorDeDocumento::novo(
            "rec_energia".into(),
            "Energia".into(),
            Some(IdOcorrencia {
                recorrente: "rec_energia".into(),
                competencia: Competencia { ano: 2026, mes: 11 },
                parcela: None,
            }),
            Competencia { ano: 2026, mes: 11 },
        );
        s.candidatos = vec![
            Candidato {
                sha256: "h1".into(),
                nome: "fatura-nov.pdf".into(),
                tipo: "fatura_cartao".into(),
                data: Some("2026-11-10".into()),
                fornecedor: None,
            },
            Candidato {
                sha256: "h2".into(),
                nome: "boleto-nov.pdf".into(),
                tipo: "boleto".into(),
                data: Some("2026-11-12".into()),
                fornecedor: None,
            },
        ];
        s
    }

    #[test]
    fn filtro_olha_perto_do_mes_da_linha() {
        let mut s = seletor();
        let f = s.filtro();
        assert_eq!(f.ciclos, Some(("2026-09".into(), "2026-12".into())));
        assert_eq!(f.texto, None);
        s.texto = " energia ".into();
        s.filtrar_por_periodo = false;
        let f = s.filtro();
        assert_eq!(f.ciclos, None);
        assert_eq!(f.texto.as_deref(), Some("energia"));
    }

    #[test]
    fn filtrar_por_tipo_esconde_o_que_nao_e_do_tipo() {
        let mut s = seletor();
        s.tipo = TipoDeDocumento::Boleto;
        assert_eq!(s.visiveis().len(), 1);
        assert_eq!(s.visiveis()[0].sha256, "h2");
        s.filtrar_por_tipo = false;
        assert_eq!(s.visiveis().len(), 2);
    }

    #[test]
    fn escolher_segue_o_tipo_do_achado_ate_o_usuario_escolher() {
        let mut s = seletor();
        s.escolher("h2");
        assert_eq!(s.tipo, TipoDeDocumento::Boleto);
        s.tipo = TipoDeDocumento::Comprovante;
        s.tipo_escolhido_a_mao = true;
        s.escolher("h1");
        assert_eq!(s.tipo, TipoDeDocumento::Comprovante);
    }

    #[test]
    fn vincula_a_ocorrencia_ou_ao_compromisso_inteiro() {
        let mut s = seletor();
        assert!(s.alteracao().is_err());
        s.escolher("h1");
        s.ao_compromisso_inteiro = false;
        let Ok(Alteracao::VincularDocumento {
            alvo,
            sha256,
            tipo_documento,
            data,
            ..
        }) = s.alteracao()
        else {
            panic!("devia vincular");
        };
        assert!(matches!(alvo, AlvoDoDocumento::Ocorrencia { .. }));
        assert_eq!(sha256, "h1");
        assert_eq!(tipo_documento, TipoDeDocumento::Fatura);
        assert_eq!(data, Data::de_iso("2026-11-10"));
        s.ao_compromisso_inteiro = true;
        assert!(matches!(
            s.alteracao(),
            Ok(Alteracao::VincularDocumento {
                alvo: AlvoDoDocumento::Recorrente { .. },
                ..
            })
        ));
    }

    #[test]
    fn linhas_de_documentos_dizem_o_alvo_e_escondem_desvinculados() {
        let doc = |id: &str, alvo, desvinculado| DocumentoVinculado {
            id: id.into(),
            alvo,
            sha256: "h".into(),
            tipo: TipoDeDocumento::Fatura,
            nome: "f.pdf".into(),
            data: Data::de_iso("2026-11-10"),
            quando: String::new(),
            autor: sbf_planejamento::Autor::Usuario,
            desvinculado,
        };
        let linhas = linhas_de_documentos(&[
            doc("a", AlvoDoDocumento::Recorrente { id: "r".into() }, false),
            doc(
                "b",
                AlvoDoDocumento::Ocorrencia {
                    id: IdOcorrencia {
                        recorrente: "r".into(),
                        competencia: Competencia { ano: 2026, mes: 11 },
                        parcela: None,
                    },
                },
                false,
            ),
            doc("c", AlvoDoDocumento::Recorrente { id: "r".into() }, true),
        ]);
        assert_eq!(linhas.len(), 2);
        assert_eq!(linhas[0].onde, "Compromisso inteiro");
        assert_eq!(linhas[1].onde, "Só 11/2026");
        assert_eq!(linhas[0].tipo, "Fatura");
        assert!(linhas[0].nota.unwrap().contains("não prova"));
        assert_eq!(linhas[0].data.as_deref(), Some("10/11/2026"));
    }
}
