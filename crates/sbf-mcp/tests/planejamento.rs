//! As ferramentas do controle mensal contra um planejamento falso, em memória (spec 11,
//! AC-30, AC-36). Dados fictícios.
#![allow(clippy::unwrap_used)]

#[path = "../../sbf-triagem/tests/ajuda/mod.rs"]
#[allow(unused_imports)]
mod ajuda;

use std::sync::{Arc, Mutex};

use ajuda::{Banco, banco_limpo};
use sbf_mcp::Nucleo;
use sbf_mcp::plano::*;
use sbf_mcp::servidor::Servidor;
use sbf_planejamento::*;
use serde_json::json;

// ------------------------------------------------------------------------ o falso

#[derive(Default)]
struct Estado {
    recorrentes: Vec<Recorrente>,
    propostas: Vec<Proposta>,
    /// Quantas vezes algo que escreve de verdade foi chamado.
    escritas: u32,
}

#[derive(Clone, Default)]
struct Falso(Arc<Mutex<Estado>>);

fn comp(a: i32, m: u8) -> Competencia {
    Competencia { ano: a, mes: m }
}

fn aluguel() -> Recorrente {
    Recorrente {
        id: "rec_aluguel".into(),
        nome: "Aluguel CPF 123.456.789-00".into(),
        grupo: "Essencial".into(),
        subgrupo: None,
        posicao: 1,
        natureza: Natureza::Saida,
        prazo: Prazo::SemFim,
        versoes: vec![Versao {
            id: "v1".into(),
            desde: comp(2026, 1),
            valor: Some(Centavos(150_000)),
            certeza: Certeza::Confirmado,
            fonte: Fonte::Usuario,
            frequencia: Frequencia::Mensal { dia: 5 },
            conta: Some("CX-0630".into()),
            centro: None,
            fornecedor: None,
            categoria: None,
            nota: None,
        }],
        pausas: vec![],
        cancelamento: None,
        arquivado: false,
        excluido: None,
        versao_do_registro: 7,
    }
}

fn salario() -> Recorrente {
    let mut r = aluguel();
    r.id = "rec_salario".into();
    r.nome = "Salario".into();
    r.natureza = Natureza::Entrada;
    r.posicao = 2;
    r.versoes[0].valor = Some(Centavos(500_000));
    r
}

fn ocorrencia(r: &Recorrente, c: Competencia) -> Ocorrencia {
    let v = &r.versoes[0];
    Ocorrencia {
        id: IdOcorrencia {
            recorrente: r.id.clone(),
            competencia: c,
            parcela: None,
        },
        nome: r.nome.clone(),
        grupo: r.grupo.clone(),
        subgrupo: None,
        posicao: r.posicao,
        natureza: r.natureza,
        vencimento: c.dia(5),
        vencimento_original: c.dia(5),
        previsto: v.valor,
        certeza: v.certeza,
        fonte: v.fonte,
        pago_informado: None,
        conciliado: None,
        estado: EstadoDaOcorrencia::Prevista,
        parcela: None,
        conta: v.conta.clone(),
        centro: None,
        fornecedor: None,
        versao: v.id.clone(),
        prazo_desconhecido: false,
        sugestao_de_conciliacao: false,
    }
}

impl Planejamento for Falso {
    fn controle(
        &self,
        periodo: &Periodo,
        _filtro: Filtro,
        busca: Option<&str>,
        _hoje: Data,
    ) -> Resultado<Controle> {
        let c = Competencia::de_data(periodo.inicio());
        let e = self.0.lock().unwrap();
        let mut linhas = Vec::new();
        for r in &e.recorrentes {
            for k in 0..30 {
                // 30 linhas por recorrente: dá o que paginar.
                let mut o = ocorrencia(r, c);
                o.id.parcela = Some(k + 1);
                o.nome = format!("{} {k:02}", r.nome);
                if busca.is_none_or(|b| o.nome.contains(b)) {
                    linhas.push(o);
                }
            }
        }
        Ok(Controle {
            periodo: *periodo,
            grupos: vec![],
            linhas,
            atrasadas_de_antes: vec![],
            resumo: Resumo {
                entradas_previstas: Centavos(500_000),
                saidas_de_caixa_previstas: Centavos(150_000),
                livre_apos_compromissos: Centavos(350_000),
                livre_e_estimativa: true,
                lacunas: vec!["falta o valor da conta 12345678901".into()],
                ..Resumo::default()
            },
        })
    }
    fn recorrentes(&self, _: bool) -> Resultado<Vec<Recorrente>> {
        Ok(self.0.lock().unwrap().recorrentes.clone())
    }
    fn recorrente(&self, id: &str) -> Resultado<Option<Recorrente>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .recorrentes
            .iter()
            .find(|r| r.id == id)
            .cloned())
    }
    fn grupos(&self) -> Resultado<Vec<Grupo>> {
        Ok(vec![])
    }
    fn proximas(
        &self,
        rec: &str,
        desde: Competencia,
        meses: u16,
        _: Data,
    ) -> Resultado<Vec<Ocorrencia>> {
        let r = self
            .recorrente(rec)?
            .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(rec.into()))?;
        Ok((0..i32::from(meses))
            .map(|k| ocorrencia(&r, desde.somar_meses(k)))
            .collect())
    }
    fn calendario(&self, inicio: Competencia, meses: u16, _: Data) -> Resultado<Vec<TotaisDoMes>> {
        if meses > HORIZONTE_MESES {
            return Err(ErroDoPlanejamento::Invalida("meses demais".into()));
        }
        Ok((0..i32::from(meses))
            .map(|k| TotaisDoMes {
                competencia: inicio.somar_meses(k),
                entradas: Centavos(500_000),
                saidas_de_caixa: Centavos(150_000),
                descontos_em_folha: Centavos(0),
                sem_valor: 0,
                estimado: k > 12,
            })
            .collect())
    }
    fn simular(&self, a: &Alteracao, _: Data) -> Resultado<Simulacao> {
        let (c, antes, depois) = match a {
            Alteracao::Editar {
                escopo, mudanca, ..
            } => {
                let c = match escopo {
                    Escopo::SoEsteMes(c) | Escopo::DaquiEmDiante(c) => *c,
                };
                (c, Some(Centavos(150_000)), mudanca.valor.flatten())
            }
            _ => (comp(2026, 12), None, None),
        };
        Ok(Simulacao {
            meses_afetados: vec![c, c.seguinte()],
            mudancas: vec![(c, antes, depois)],
            conflitos: vec!["pago informado de 12345678901 atingido".into()],
        })
    }
    fn aplicar(&mut self, _: Alteracao, _: Autor) -> Resultado<Recibo> {
        self.0.lock().unwrap().escritas += 1;
        Err(ErroDoPlanejamento::Banco("não deveria ser chamado".into()))
    }
    fn propor(&mut self, alteracao: Alteracao, origem: &str, chave: &str) -> Resultado<Proposta> {
        let mut e = self.0.lock().unwrap();
        if let Some(p) = e.propostas.iter().find(|p| p.chave == chave) {
            return Ok(p.clone());
        }
        let p = Proposta {
            id: format!("prop_{}", e.propostas.len() + 1),
            alteracao,
            origem: origem.into(),
            chave: chave.into(),
            criada_em: "2026-11-16T12:00:00Z".into(),
            expira_em: "2026-11-23T12:00:00Z".into(),
            estado: EstadoDaProposta::Pendente,
            motivo: None,
        };
        e.propostas.push(p.clone());
        Ok(p)
    }
    fn propostas(&self, so_pendentes: bool) -> Resultado<Vec<Proposta>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .propostas
            .iter()
            .filter(|p| !so_pendentes || p.estado == EstadoDaProposta::Pendente)
            .cloned()
            .collect())
    }
    fn confirmar_proposta(&mut self, _: &str) -> Resultado<Recibo> {
        self.0.lock().unwrap().escritas += 1;
        Err(ErroDoPlanejamento::Banco("não deveria ser chamado".into()))
    }
    fn recusar_proposta(&mut self, _: &str, _: &str) -> Resultado<()> {
        self.0.lock().unwrap().escritas += 1;
        Ok(())
    }
    fn historico(&self, _: Option<&str>, _: u32) -> Resultado<Vec<EntradaDoHistorico>> {
        Ok(vec![EntradaDoHistorico {
            id: "h1".into(),
            quando: "2026-10-01T10:00:00Z".into(),
            autor: Autor::Usuario,
            alvo: "rec_aluguel".into(),
            resumo: "valor ajustado".into(),
            alteracao: "{}".into(),
        }])
    }
    fn origem_importada(&self, _: &OrigemImportada) -> Resultado<Option<String>> {
        Ok(None)
    }
}

struct Cenario {
    banco: Banco,
    falso: Falso,
    nucleo: Nucleo,
}

fn cenario() -> Cenario {
    let banco = banco_limpo();
    let falso = Falso::default();
    falso.0.lock().unwrap().recorrentes = vec![aluguel(), salario()];
    let f = falso.clone();
    let fabrica: Arc<Fabrica> = Arc::new(move || Ok(Box::new(f.clone()) as Box<dyn Planejamento>));
    let nucleo = Nucleo::novo(&banco.caminho, None)
        .unwrap()
        .com_planejamento(fabrica)
        .com_hoje(Data {
            ano: 2026,
            mes: 11,
            dia: 16,
        });
    Cenario {
        banco,
        falso,
        nucleo,
    }
}

impl Cenario {
    fn plano(&self) -> &Plano {
        self.nucleo.plano().unwrap()
    }
    fn saidas(&self, destinatario: &str) -> i64 {
        self.banco
            .conexao()
            .query_row(
                "SELECT COUNT(*) FROM saida_ia WHERE destinatario = ?1 AND nivel = 'mascarado'",
                [destinatario],
                |l| l.get(0),
            )
            .unwrap()
    }
}

fn pausa(chave: &str) -> ComChave<PedidoDePausa> {
    ComChave {
        chave: chave.into(),
        pedido: PedidoDePausa {
            recorrente: "rec_aluguel".into(),
            desde: "2027-01".into(),
            ate: Some("2027-04".into()),
            motivo: "viagem longa".into(),
        },
    }
}

// ------------------------------------------------------------------------ testes

#[test]
fn lista_de_ferramentas_so_tem_planejamento_quando_ligado() {
    let c = cenario();
    let nomes = Servidor::novo(c.nucleo.clone()).nomes_das_ferramentas();
    for n in FERRAMENTAS {
        assert!(nomes.iter().any(|x| x == n), "falta {n}");
    }
    assert!(nomes.iter().any(|n| n == "sugerir"), "as antigas continuam");

    let banco = banco_limpo();
    let sem = Nucleo::novo(&banco.caminho, None).unwrap();
    let nomes = Servidor::novo(sem).nomes_das_ferramentas();
    assert!(nomes.iter().any(|n| n == "inventario_resumo"));
    for n in FERRAMENTAS {
        assert!(!nomes.iter().any(|x| x == n), "{n} não devia aparecer");
    }
}

#[test]
fn ac30_nenhuma_ferramenta_expoe_aplicar_confirmar_recusar_ou_sql() {
    let c = cenario();
    let nomes = Servidor::novo(c.nucleo.clone()).nomes_das_ferramentas();
    for n in &nomes {
        for proibido in [
            "aplicar",
            "confirmar",
            "recusar",
            "sql",
            "executar",
            "excluir_",
            "pagar",
        ] {
            assert!(!n.contains(proibido), "ferramenta proibida: {n}");
        }
    }
}

#[test]
fn ac30_so_propor_cria_proposta_e_nada_mais_muda() {
    let c = cenario();
    let p = c.plano();
    let antes = c.falso.0.lock().unwrap().recorrentes.clone();

    // Leituras e simulação não criam nada.
    p.listar_compromissos(&PedidoDeLista::default()).unwrap();
    p.resumo_de_caixa(&PedidoDePeriodo::default()).unwrap();
    p.projetar_compromissos(&PedidoDeProjecao::default())
        .unwrap();
    p.obter_compromisso(&PedidoDeCompromisso {
        id_ocorrencia: "rec_aluguel:2026-12".into(),
        proximas: None,
    })
    .unwrap();
    p.simular_mudanca(&PedidoDeSimulacao {
        alteracao: EntradaDeAlteracao::Pausa(pausa("x").pedido),
    })
    .unwrap();
    p.listar_propostas(&PedidoDePropostas::default()).unwrap();
    assert!(c.falso.0.lock().unwrap().propostas.is_empty());

    let r = p
        .propor(
            "propor_pausa",
            "k1",
            &EntradaDeAlteracao::Pausa(pausa("k1").pedido),
        )
        .unwrap();
    assert_eq!(
        r["mensagem"],
        "proposta pendente: confirme na janela do SBF"
    );
    assert_eq!(r["proposta"]["estado"], "pendente");
    assert!(
        !r["simulacao"]["meses_afetados"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let e = c.falso.0.lock().unwrap();
    assert_eq!(e.propostas.len(), 1);
    assert_eq!(e.propostas[0].origem, "mcp");
    assert_eq!(e.escritas, 0, "ninguém chamou aplicar/confirmar/recusar");
    assert_eq!(e.recorrentes, antes, "os recorrentes não mudaram");
}

#[test]
fn proposta_e_idempotente_pela_chave() {
    let c = cenario();
    let p = c.plano();
    let e = EntradaDeAlteracao::Pausa(pausa("k").pedido);
    let a = p.propor("propor_pausa", "chave-1", &e).unwrap();
    let b = p.propor("propor_pausa", "chave-1", &e).unwrap();
    assert_eq!(a["proposta"]["id"], b["proposta"]["id"]);
    assert!(b.get("aviso").is_none());
    assert_eq!(c.falso.0.lock().unwrap().propostas.len(), 1);

    // Mesma chave com outro conteúdo: nada novo, e o Claude é avisado.
    let outra = EntradaDeAlteracao::Retomada(PedidoDeRetomada {
        recorrente: "rec_aluguel".into(),
        desde: "2027-02".into(),
    });
    let r = p.propor("propor_retomada", "chave-1", &outra).unwrap();
    assert!(r["aviso"].as_str().unwrap().contains("outra proposta"));
    assert_eq!(c.falso.0.lock().unwrap().propostas.len(), 1);
}

#[test]
fn chave_e_obrigatoria_e_limitada() {
    let c = cenario();
    let e = EntradaDeAlteracao::Pausa(pausa("k").pedido);
    for ruim in ["", "   ", &"x".repeat(121)] {
        assert!(c.plano().propor("propor_pausa", ruim, &e).is_err());
    }
    assert!(c.falso.0.lock().unwrap().propostas.is_empty());
    // O esquema exige `chave`.
    let sem_chave = json!({ "recorrente": "rec_aluguel", "desde": "2027-01", "motivo": "m" });
    assert!(serde_json::from_value::<ComChave<PedidoDePausa>>(sem_chave).is_err());
    let com =
        json!({ "chave": "k", "recorrente": "rec_aluguel", "desde": "2027-01", "motivo": "m" });
    assert!(serde_json::from_value::<ComChave<PedidoDePausa>>(com).is_ok());
}

#[test]
fn listagem_pagina_e_valida_limites() {
    let c = cenario();
    let p = c.plano();
    let r = p
        .listar_compromissos(&PedidoDeLista {
            periodo: Some("mes".into()),
            referencia: Some("2026-11".into()),
            por_pagina: Some(25),
            ..PedidoDeLista::default()
        })
        .unwrap();
    assert_eq!(r["total"], 60);
    assert_eq!(r["itens"].as_array().unwrap().len(), 25);
    assert_eq!(r["total_de_paginas"], 3);
    assert_eq!(r["periodo"]["inicio"], "2026-11-01");
    let ultima = p
        .listar_compromissos(&PedidoDeLista {
            pagina: Some(3),
            por_pagina: Some(25),
            ..PedidoDeLista::default()
        })
        .unwrap();
    assert_eq!(ultima["itens"].as_array().unwrap().len(), 10);

    for ruim in [
        PedidoDeLista {
            por_pagina: Some(101),
            ..Default::default()
        },
        PedidoDeLista {
            por_pagina: Some(0),
            ..Default::default()
        },
        PedidoDeLista {
            pagina: Some(0),
            ..Default::default()
        },
        PedidoDeLista {
            filtro: Some("qualquer".into()),
            ..Default::default()
        },
        PedidoDeLista {
            periodo: Some("ano".into()),
            ..Default::default()
        },
        PedidoDeLista {
            referencia: Some("2026-13".into()),
            ..Default::default()
        },
        PedidoDeLista {
            busca: Some("x".repeat(101)),
            ..Default::default()
        },
    ] {
        let e = p.listar_compromissos(&ruim).unwrap_err();
        assert!(!e.is_empty());
    }
    // Ciclo por padrão: hoje (16/11) cai no ciclo que começou em 20/10.
    let r = p.listar_compromissos(&PedidoDeLista::default()).unwrap();
    assert_eq!(r["periodo"]["tipo"], "ciclo");
    assert_eq!(r["periodo"]["inicio"], "2026-10-20");
}

#[test]
fn projecao_de_360_meses_e_limite() {
    let c = cenario();
    let p = c.plano();
    let r = p
        .projetar_compromissos(&PedidoDeProjecao {
            inicio: Some("2026-11".into()),
            meses: Some(360),
        })
        .unwrap();
    assert_eq!(r["meses"], 360);
    let t = r["totais"].as_array().unwrap();
    assert_eq!(t.len(), 360);
    assert_eq!(t[0]["competencia"], "2026-11");
    assert_eq!(t[359]["competencia"], "2056-10");
    assert_eq!(r["soma"]["entradas_centavos"], 360 * 500_000);
    assert!(
        p.projetar_compromissos(&PedidoDeProjecao {
            inicio: None,
            meses: Some(361)
        })
        .unwrap_err()
        .contains("360")
    );
    assert!(
        p.projetar_compromissos(&PedidoDeProjecao {
            inicio: None,
            meses: Some(0)
        })
        .is_err()
    );
}

#[test]
fn obter_compromisso_por_ocorrencia_e_por_recorrente() {
    let c = cenario();
    let p = c.plano();
    let a = p
        .obter_compromisso(&PedidoDeCompromisso {
            id_ocorrencia: "rec_aluguel:2026-12".into(),
            proximas: Some(3),
        })
        .unwrap();
    assert_eq!(a["proximas_ocorrencias"].as_array().unwrap().len(), 3);
    assert_eq!(a["ocorrencia"]["id"], "rec_aluguel:2026-12");
    assert_eq!(a["historico"].as_array().unwrap().len(), 1);
    assert_eq!(a["compromisso"]["versoes"][0]["desde"], "2026-01");
    let b = p
        .obter_compromisso(&PedidoDeCompromisso {
            id_ocorrencia: "rec_aluguel".into(),
            proximas: None,
        })
        .unwrap();
    assert!(b["ocorrencia"].is_null());
    assert_eq!(b["proximas_ocorrencias"].as_array().unwrap().len(), 12);
    let e = p
        .obter_compromisso(&PedidoDeCompromisso {
            id_ocorrencia: "rec_nada".into(),
            proximas: None,
        })
        .unwrap_err();
    assert!(e.contains("não encontrei"));
}

#[test]
fn simulacao_mostra_antes_e_depois_sem_gravar() {
    let c = cenario();
    let r = c
        .plano()
        .simular_mudanca(&PedidoDeSimulacao {
            alteracao: EntradaDeAlteracao::Alteracao(PedidoDeAlteracao {
                recorrente: "rec_aluguel".into(),
                escopo: "daqui_em_diante".into(),
                competencia: "2027-03".into(),
                valor_centavos: Some(160_000),
                sem_valor: None,
                certeza: None,
                conta: None,
                centro: None,
                fornecedor: None,
                categoria: None,
                nota: None,
                limpar: None,
            }),
        })
        .unwrap();
    assert_eq!(r["grava"], false);
    assert_eq!(r["simulacao"]["meses_afetados"][0], "2027-03");
    let m = &r["simulacao"]["mudancas"][0];
    assert_eq!(m["antes_centavos"], 150_000);
    assert_eq!(m["depois_centavos"], 160_000);
    assert!(c.falso.0.lock().unwrap().propostas.is_empty());
}

#[test]
fn argumentos_de_alteracao_sao_validados_em_portugues() {
    let c = cenario();
    let base = || PedidoDeAlteracao {
        recorrente: "rec_aluguel".into(),
        escopo: "so_este_mes".into(),
        competencia: "2027-03".into(),
        valor_centavos: Some(1000),
        sem_valor: None,
        certeza: None,
        conta: None,
        centro: None,
        fornecedor: None,
        categoria: None,
        nota: None,
        limpar: None,
    };
    let propor = |a: PedidoDeAlteracao| {
        c.plano()
            .propor("propor_alteracao", "k", &EntradaDeAlteracao::Alteracao(a))
    };
    assert!(
        propor(PedidoDeAlteracao {
            nota: Some("x".into()),
            ..base()
        })
        .unwrap_err()
        .contains("so_este_mes")
    );
    assert!(
        propor(PedidoDeAlteracao {
            valor_centavos: Some(-1),
            ..base()
        })
        .is_err()
    );
    assert!(
        propor(PedidoDeAlteracao {
            escopo: "sempre".into(),
            ..base()
        })
        .is_err()
    );
    assert!(
        propor(PedidoDeAlteracao {
            nota: Some("x".repeat(501)),
            escopo: "daqui_em_diante".into(),
            ..base()
        })
        .is_err()
    );
    assert!(
        propor(PedidoDeAlteracao {
            recorrente: "rec_nada".into(),
            ..base()
        })
        .unwrap_err()
        .contains("não encontrei")
    );
    assert!(
        propor(PedidoDeAlteracao {
            valor_centavos: None,
            ..base()
        })
        .unwrap_err()
        .contains("diga o que mudar")
    );
    assert!(c.falso.0.lock().unwrap().propostas.is_empty());
    // Válida: usa a versão de base atual do recorrente (AC-31 se confirma depois).
    propor(base()).unwrap();
    let e = c.falso.0.lock().unwrap();
    match &e.propostas[0].alteracao {
        Alteracao::Editar { versao_base, .. } => assert_eq!(*versao_base, 7),
        outra => panic!("esperava Editar, veio {outra:?}"),
    }
}

#[test]
fn propostas_de_todos_os_tipos_viram_alteracoes_certas() {
    let c = cenario();
    let p = c.plano();
    let lista: Vec<(&str, EntradaDeAlteracao)> = vec![
        (
            "propor_compromisso",
            EntradaDeAlteracao::Compromisso(PedidoNovoCompromisso {
                nome: "Curso".into(),
                grupo: "Lar".into(),
                subgrupo: None,
                posicao: None,
                natureza: "saida".into(),
                valor_centavos: Some(9900),
                certeza: None,
                desde: "2026-12".into(),
                frequencia: None,
                dia: Some(10),
                a_cada_n_meses: None,
                mes_do_ano: None,
                data: None,
                prazo: Some("parcelas".into()),
                parcelas: Some(12),
                ate_competencia: None,
                conta: None,
                centro: None,
                fornecedor: None,
                categoria: None,
                nota: None,
            }),
        ),
        (
            "propor_retomada",
            EntradaDeAlteracao::Retomada(PedidoDeRetomada {
                recorrente: "rec_aluguel".into(),
                desde: "2027-02".into(),
            }),
        ),
        (
            "propor_cancelamento",
            EntradaDeAlteracao::Cancelamento(PedidoDeCancelamento {
                recorrente: "rec_aluguel".into(),
                desde: "2027-02".into(),
                motivo: "mudei".into(),
                comprovado: None,
            }),
        ),
        (
            "propor_exclusao_logica",
            EntradaDeAlteracao::ExclusaoLogica(PedidoDeExclusao {
                recorrente: "rec_aluguel".into(),
                motivo: "duplicado".into(),
            }),
        ),
        (
            "propor_conciliacao",
            EntradaDeAlteracao::Conciliacao(PedidoDeConciliacao {
                id_ocorrencia: "rec_aluguel:2026-11".into(),
                movimento: "pluggy:txn_abc".into(),
                valor_centavos: 150_000,
                data: "2026-11-05".into(),
            }),
        ),
        (
            "propor_excecao",
            EntradaDeAlteracao::Excecao(PedidoDeExcecao {
                id_ocorrencia: "rec_aluguel:2026-11".into(),
                acao: "adiar".into(),
                nova_data: Some("2026-11-20".into()),
                motivo: None,
                valor_centavos: None,
                data: None,
            }),
        ),
    ];
    for (i, (nome, e)) in lista.iter().enumerate() {
        p.propor(nome, &format!("chave-{i}"), e).unwrap();
    }
    let e = c.falso.0.lock().unwrap();
    assert_eq!(e.propostas.len(), lista.len());
    assert!(
        matches!(&e.propostas[0].alteracao, Alteracao::CriarRecorrente { recorrente, .. }
        if matches!(recorrente.prazo, Prazo::Parcelas { total: 12, .. })
            && recorrente.primeira_versao.fonte == Fonte::Ia)
    );
    assert!(matches!(
        &e.propostas[2].alteracao,
        Alteracao::Cancelar {
            comprovado: false,
            ..
        }
    ));
    // A IA só sugere conciliação: nunca confirmada.
    assert!(
        matches!(&e.propostas[4].alteracao, Alteracao::Conciliar { conciliacao } if !conciliacao.confirmada)
    );
}

#[test]
fn texto_livre_sai_mascarado_e_datas_ficam_legiveis() {
    let c = cenario();
    let r = c
        .plano()
        .listar_compromissos(&PedidoDeLista::default())
        .unwrap();
    let nome = r["itens"][0]["nome"].as_str().unwrap();
    assert!(!nome.contains("123.456.789-00"), "CPF vazou: {nome}");
    assert!(nome.contains("•••"));
    let resumo = c
        .plano()
        .resumo_de_caixa(&PedidoDePeriodo::default())
        .unwrap();
    let lacuna = resumo["resumo"]["lacunas"][0].as_str().unwrap();
    assert!(!lacuna.contains("12345678901"));
    // Valores e ids não são mexidos.
    assert_eq!(resumo["resumo"]["entradas_previstas"], 500_000);
    let s = c
        .plano()
        .simular_mudanca(&PedidoDeSimulacao {
            alteracao: EntradaDeAlteracao::Pausa(pausa("k").pedido),
        })
        .unwrap();
    assert!(
        !s["simulacao"]["conflitos"][0]
            .as_str()
            .unwrap()
            .contains("12345678901")
    );
}

#[test]
fn cada_chamada_grava_uma_linha_em_saida_ia_sem_conteudo() {
    let c = cenario();
    let p = c.plano();
    p.listar_compromissos(&PedidoDeLista::default()).unwrap();
    p.listar_compromissos(&PedidoDeLista::default()).unwrap();
    assert_eq!(c.saidas("mcp:listar_compromissos"), 2);
    p.propor(
        "propor_pausa",
        "k",
        &EntradaDeAlteracao::Pausa(pausa("k").pedido),
    )
    .unwrap();
    assert_eq!(c.saidas("mcp:propor_pausa"), 1);
    // Chamada que erra também deixa a linha.
    let _ = p.projetar_compromissos(&PedidoDeProjecao {
        inicio: None,
        meses: Some(999),
    });
    assert_eq!(c.saidas("mcp:projetar_compromissos"), 1);
    // A linha guarda só quem, nível e quantos bytes.
    let colunas: Vec<String> = c
        .banco
        .conexao()
        .prepare("SELECT name FROM pragma_table_info('saida_ia')")
        .unwrap()
        .query_map([], |l| l.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        !colunas
            .iter()
            .any(|n| n.contains("conteudo") || n.contains("texto"))
    );
}

#[test]
fn sem_planejamento_as_chamadas_dizem_por_que() {
    let banco = banco_limpo();
    let n = Nucleo::novo(&banco.caminho, None).unwrap();
    assert!(n.plano().unwrap_err().contains("planejamento"));
}
