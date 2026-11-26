//! Modelo de compromisso e Cálculo (diarista por dia da semana). Dados fictícios; o Luau é o
//! de verdade (`ExecutorNoProcesso`), embrulhado num contador de pedidos.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use sbf_planejamento::{
    Alteracao, Autor, Banco, Centavos, Certeza, Competencia, Data, ErroDoPlanejamento, Excecao,
    FABRICA_NOME_DO_MODELO, Fonte, Frequencia, IdOcorrencia, Natureza, NovoRecorrente, Ocorrencia,
    ParametroDeModelo, Planejamento, PlanejamentoSqlite, Prazo, SemSaldos, TipoDeParametro,
    ValorDeParametro, ValorDoParametro, Versao,
};
use sbf_regras::script::{
    ErroDoScript, ExecutorDeScript, PedidoDeCalculo, PedidoDeScript, RespostaDoScript,
    ResultadoDeCalculo,
};
use sbf_script::ExecutorNoProcesso;

const HOJE: Data = Data {
    ano: 2026,
    mes: 11,
    dia: 16,
};

fn comp(ano: i32, mes: u8) -> Competencia {
    Competencia { ano, mes }
}

/// O Luau de verdade, contando quantos lotes e quantos meses foram ao executor.
#[derive(Debug, Default)]
struct Contador {
    dentro: ExecutorNoProcesso,
    lotes: AtomicUsize,
    meses: AtomicUsize,
}

impl ExecutorDeScript for Contador {
    fn executar(&self, p: &PedidoDeScript) -> Result<RespostaDoScript, ErroDoScript> {
        self.dentro.executar(p)
    }
    fn verificar(&self, f: &str) -> Result<(), ErroDoScript> {
        self.dentro.verificar(f)
    }
    fn calcular(&self, p: &PedidoDeCalculo) -> Result<Vec<ResultadoDeCalculo>, ErroDoScript> {
        self.lotes.fetch_add(1, Ordering::SeqCst);
        self.meses.fetch_add(p.itens.len(), Ordering::SeqCst);
        self.dentro.calcular(p)
    }
    fn verificar_calculo(&self, f: &str) -> Result<(), ErroDoScript> {
        self.dentro.verificar_calculo(f)
    }
}

impl Contador {
    fn meses(&self) -> usize {
        self.meses.load(Ordering::SeqCst)
    }
    fn lotes(&self) -> usize {
        self.lotes.load(Ordering::SeqCst)
    }
}

fn servico_com(executor: Arc<Contador>) -> PlanejamentoSqlite {
    PlanejamentoSqlite::novo(Banco::em_memoria().expect("banco"), Box::new(SemSaldos))
        .com_calculo(executor)
}

fn servico() -> (PlanejamentoSqlite, Arc<Contador>) {
    let ex = Arc::new(Contador::default());
    (servico_com(Arc::clone(&ex)), ex)
}

/// Um compromisso mensal sem fim, desde 2025-01, com o valor "último conhecido" de R$ 1.000.
fn criar(s: &mut PlanejamentoSqlite, nome: &str, autor: Autor) -> String {
    s.aplicar(
        Alteracao::CriarRecorrente {
            recorrente: NovoRecorrente {
                nome: nome.into(),
                grupo: "Casa".into(),
                subgrupo: None,
                posicao: 1,
                natureza: Natureza::Saida,
                prazo: Prazo::SemFim,
                primeira_versao: Versao {
                    id: String::new(),
                    desde: comp(2025, 1),
                    valor: Some(Centavos(100_000)),
                    certeza: Certeza::Estimado,
                    fonte: Fonte::Usuario,
                    frequencia: Frequencia::Mensal { dia: 5 },
                    conta: None,
                    centro: None,
                    fornecedor: None,
                    categoria: None,
                    nota: None,
                },
            },
            origem: None,
        },
        autor,
    )
    .expect("criar")
    .alvo
}

fn base(s: &PlanejamentoSqlite, id: &str) -> i64 {
    s.recorrente(id)
        .expect("ler")
        .expect("existe")
        .versao_do_registro
}

fn id_do_modelo(s: &PlanejamentoSqlite) -> String {
    s.modelos()
        .expect("modelos")
        .into_iter()
        .find(|m| m.nome == FABRICA_NOME_DO_MODELO)
        .expect("modelo de fábrica")
        .id
}

fn valor(parametro: &str, desde: Competencia, v: i64) -> ValorDeParametro {
    ValorDeParametro {
        parametro: parametro.into(),
        desde,
        valor: ValorDoParametro::Inteiro(v),
    }
}

/// Liga o compromisso ao modelo de fábrica: diária R$ 190 até 2026-04, R$ 210 desde 2026-05,
/// sempre às sextas (dia 5).
fn ligar_a_diarista(s: &mut PlanejamentoSqlite, rec: &str, autor: Autor) -> String {
    let modelo = id_do_modelo(s);
    let b = base(s, rec);
    s.aplicar(
        Alteracao::AplicarModelo {
            recorrente: rec.into(),
            modelo,
            valores: vec![
                valor("diaria", comp(2026, 1), 19_000),
                valor("diaria", comp(2026, 5), 21_000),
                valor("dia_da_semana", comp(2026, 1), 5),
            ],
            versao_base: b,
        },
        autor,
    )
    .expect("aplicar modelo")
    .alteracao_id
}

fn mes(s: &PlanejamentoSqlite, rec: &str, c: Competencia) -> Ocorrencia {
    s.proximas(rec, c, 1, HOJE)
        .expect("proximas")
        .into_iter()
        .next()
        .expect("ocorrência")
}

fn previsto(s: &PlanejamentoSqlite, rec: &str, c: Competencia) -> Option<i64> {
    mes(s, rec, c).previsto.map(|v| v.0)
}

#[test]
fn a_fabrica_traz_o_modelo_diarista_com_seus_parametros() {
    let (s, _) = servico();
    let modelos = s.modelos().expect("modelos");
    let m = modelos
        .iter()
        .find(|m| m.nome == FABRICA_NOME_DO_MODELO)
        .expect("de fábrica");
    assert!(m.de_fabrica);
    assert_eq!(m.versao_do_calculo, 1);
    assert_eq!(m.checksum.len(), 64);
    assert_eq!(
        m.parametros,
        vec![
            ParametroDeModelo {
                nome: "diaria".into(),
                tipo: TipoDeParametro::Centavos
            },
            ParametroDeModelo {
                nome: "dia_da_semana".into(),
                tipo: TipoDeParametro::DiaDaSemana
            },
        ]
    );
    // Abrir de novo não duplica.
    assert_eq!(s.modelos().expect("modelos").len(), 1);
}

#[test]
fn diaria_vezes_sextas_em_meses_com_quatro_e_cinco() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &rec, Autor::Usuario);
    // Abril/2026: 4 sextas × R$ 190. Maio/2026: 5 sextas × R$ 210 (a diária mudou na vigência).
    assert_eq!(previsto(&s, &rec, comp(2026, 4)), Some(76_000));
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(105_000));
    // Janeiro/2026 tem 5 sextas.
    assert_eq!(previsto(&s, &rec, comp(2026, 1)), Some(95_000));
    let o = mes(&s, &rec, comp(2026, 5));
    let k = o.calculo.expect("cálculo");
    assert_eq!(k.modelo, FABRICA_NOME_DO_MODELO);
    assert_eq!(k.explicacao, "diária R$ 210,00 × 5 sextas");
    assert_eq!(k.quantidade, Some(5));
    assert!(!k.ajustada);
    assert_eq!(k.aviso, None);
    // Antes do primeiro valor o compromisso ainda não usava o modelo: vale o da versão.
    assert_eq!(previsto(&s, &rec, comp(2025, 12)), Some(100_000));
    assert_eq!(mes(&s, &rec, comp(2025, 12)).calculo, None);
}

#[test]
fn trocar_a_diaria_na_vigencia_so_muda_dali_em_diante() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    let modelo = id_do_modelo(&s);
    let b = base(&s, &rec);
    s.aplicar(
        Alteracao::AplicarModelo {
            recorrente: rec.clone(),
            modelo,
            valores: vec![
                valor("diaria", comp(2026, 1), 19_000),
                valor("dia_da_semana", comp(2026, 1), 5),
            ],
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("aplicar");
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(95_000));
    let b = base(&s, &rec);
    s.aplicar(
        Alteracao::DefinirParametro {
            recorrente: rec.clone(),
            valor: valor("diaria", comp(2026, 5), 21_000),
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("parâmetro");
    assert_eq!(previsto(&s, &rec, comp(2026, 4)), Some(76_000));
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(105_000));
    // Junho/2026 tem 4 sextas.
    assert_eq!(previsto(&s, &rec, comp(2026, 6)), Some(84_000));
    let l = s.ligacao_de_modelo(&rec).expect("ligação").expect("há");
    assert_eq!(l.valores.len(), 3);
    assert_eq!(l.versao_do_calculo, 1);
}

#[test]
fn ajustar_a_quantidade_vale_so_para_aquele_mes_e_o_rastro_diz_quem() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &rec, Autor::Usuario);
    let jan = IdOcorrencia {
        recorrente: rec.clone(),
        competencia: comp(2026, 1),
        parcela: None,
    };
    let recibo = s
        .aplicar(
            Alteracao::Excecao {
                ocorrencia: jan,
                excecao: Excecao::QuantidadeSoDesteMes { quantidade: 6 },
            },
            Autor::Mcp,
        )
        .expect("ajuste");
    // Janeiro: 6 diárias × R$ 190; fevereiro segue o cálculo (4 sextas).
    assert_eq!(previsto(&s, &rec, comp(2026, 1)), Some(114_000));
    let k = mes(&s, &rec, comp(2026, 1)).calculo.expect("cálculo");
    assert!(k.ajustada);
    assert_eq!(k.quantidade, Some(6));
    assert_eq!(k.explicacao, "diária R$ 190,00 × 6 sextas");
    assert_eq!(previsto(&s, &rec, comp(2026, 2)), Some(76_000));
    assert!(!mes(&s, &rec, comp(2026, 2)).calculo.expect("c").ajustada);
    // O rastro mostra quem ajustou.
    let excecoes = s.excecoes_do_recorrente(&rec).expect("exceções");
    let e = excecoes
        .iter()
        .find(|e| matches!(e.excecao, Excecao::QuantidadeSoDesteMes { .. }))
        .expect("exceção");
    assert_eq!(e.autor, Autor::Mcp);
    // E o Desfazer volta ao cálculo.
    s.desfazer(&recibo.alteracao_id, Autor::Mcp)
        .expect("desfazer");
    assert_eq!(previsto(&s, &rec, comp(2026, 1)), Some(95_000));
    assert!(!mes(&s, &rec, comp(2026, 1)).calculo.expect("c").ajustada);
}

#[test]
fn um_novo_ajuste_do_mesmo_mes_substitui_o_anterior() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &rec, Autor::Usuario);
    for q in [6, 3] {
        s.aplicar(
            Alteracao::Excecao {
                ocorrencia: IdOcorrencia {
                    recorrente: rec.clone(),
                    competencia: comp(2026, 1),
                    parcela: None,
                },
                excecao: Excecao::QuantidadeSoDesteMes { quantidade: q },
            },
            Autor::Usuario,
        )
        .expect("ajuste");
    }
    assert_eq!(previsto(&s, &rec, comp(2026, 1)), Some(57_000));
}

#[test]
fn quantidade_so_vale_em_compromisso_com_modelo_e_dentro_do_intervalo() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Sem Modelo Exemplo", Autor::Usuario);
    let id = IdOcorrencia {
        recorrente: rec.clone(),
        competencia: comp(2026, 1),
        parcela: None,
    };
    let sem_modelo = s.aplicar(
        Alteracao::Excecao {
            ocorrencia: id.clone(),
            excecao: Excecao::QuantidadeSoDesteMes { quantidade: 3 },
        },
        Autor::Usuario,
    );
    assert!(matches!(sem_modelo, Err(ErroDoPlanejamento::Invalida(_))));
    ligar_a_diarista(&mut s, &rec, Autor::Usuario);
    for ruim in [-1, 1001] {
        let r = s.aplicar(
            Alteracao::Excecao {
                ocorrencia: id.clone(),
                excecao: Excecao::QuantidadeSoDesteMes { quantidade: ruim },
            },
            Autor::Usuario,
        );
        assert!(matches!(r, Err(ErroDoPlanejamento::Invalida(_))), "{ruim}");
    }
}

#[test]
fn desfazer_cobre_a_ligacao_o_parametro_e_a_criacao_do_modelo() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    // Ligar e desfazer: volta ao valor da versão, sem cálculo.
    let id_da_ligacao = ligar_a_diarista(&mut s, &rec, Autor::Mcp);
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(105_000));
    // Mudar um parâmetro e desfazer: volta ao anterior.
    let b = base(&s, &rec);
    let r = s
        .aplicar(
            Alteracao::DefinirParametro {
                recorrente: rec.clone(),
                valor: valor("diaria", comp(2026, 5), 30_000),
                versao_base: b,
            },
            Autor::Mcp,
        )
        .expect("parâmetro");
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(150_000));
    s.desfazer(&r.alteracao_id, Autor::Mcp)
        .expect("desfazer parâmetro");
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(105_000));
    // O último Desfazer do MCP agora é o que desfaz esse desfazer? Não: um Desfazer não se
    // desfaz. A ligação é que se desfaz.
    s.desfazer(&id_da_ligacao, Autor::Mcp)
        .expect("desfazer ligação");
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(100_000));
    assert_eq!(mes(&s, &rec, comp(2026, 5)).calculo, None);
    assert_eq!(s.ligacao_de_modelo(&rec).expect("ligação"), None);
    // Já desfeita.
    assert!(s.desfazer(&id_da_ligacao, Autor::Mcp).is_err());
}

#[test]
fn desfazer_a_ligacao_por_cima_de_outra_volta_ao_modelo_anterior() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &rec, Autor::Usuario);
    // Um segundo modelo, com o script mais simples possível.
    let novo = s
        .aplicar(
            Alteracao::CriarModelo {
                nome: "Fixo de teste".into(),
                parametros: vec![ParametroDeModelo {
                    nome: "valor".into(),
                    tipo: TipoDeParametro::Centavos,
                }],
                calculo: "function calcular(e, a) return {valor_centavos = e.parametros.valor, explicacao = 'fixo'} end".into(),
            },
            Autor::Usuario,
        )
        .expect("criar modelo");
    let b = base(&s, &rec);
    let r = s
        .aplicar(
            Alteracao::AplicarModelo {
                recorrente: rec.clone(),
                modelo: novo.alvo.clone(),
                valores: vec![valor("valor", comp(2026, 1), 7_000)],
                versao_base: b,
            },
            Autor::Mcp,
        )
        .expect("trocar de modelo");
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(7_000));
    s.desfazer(&r.alteracao_id, Autor::Mcp).expect("desfazer");
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(105_000));
    assert_eq!(
        s.ligacao_de_modelo(&rec).expect("l").expect("há").modelo,
        FABRICA_NOME_DO_MODELO
    );
}

#[test]
fn criar_modelo_valida_e_desfaz_e_o_nome_fica_livre_de_novo() {
    let (mut s, _) = servico();
    let parametros = vec![ParametroDeModelo {
        nome: "valor".into(),
        tipo: TipoDeParametro::Centavos,
    }];
    let calculo = "function calcular(e, a) return {valor_centavos = 1, explicacao = 'x'} end";
    let novo =
        |nome: &str, parametros: Vec<ParametroDeModelo>, calculo: &str| Alteracao::CriarModelo {
            nome: nome.into(),
            parametros,
            calculo: calculo.into(),
        };
    // Script sem `calcular`, com erro de sintaxe, nome repetido, parâmetro mal escrito.
    for ruim in [
        novo("A", parametros.clone(), "x = 1"),
        novo("A", parametros.clone(), "function calcular("),
        novo(FABRICA_NOME_DO_MODELO, parametros.clone(), calculo),
        novo("", parametros.clone(), calculo),
        novo("A", vec![], calculo),
        novo(
            "A",
            vec![ParametroDeModelo {
                nome: "Diária".into(),
                tipo: TipoDeParametro::Texto,
            }],
            calculo,
        ),
        novo(
            "A",
            vec![parametros[0].clone(), parametros[0].clone()],
            calculo,
        ),
    ] {
        assert!(
            matches!(
                s.aplicar(ruim, Autor::Mcp),
                Err(ErroDoPlanejamento::Invalida(_))
            ),
            "deveria recusar"
        );
    }
    let r = s
        .aplicar(novo("Meu modelo", parametros.clone(), calculo), Autor::Mcp)
        .expect("criar");
    assert_eq!(s.modelos().expect("m").len(), 2);
    s.desfazer(&r.alteracao_id, Autor::Mcp).expect("desfazer");
    assert_eq!(s.modelos().expect("m").len(), 1);
    s.aplicar(novo("Meu modelo", parametros, calculo), Autor::Mcp)
        .expect("de novo");
    assert_eq!(s.modelos().expect("m").len(), 2);
}

#[test]
fn valores_de_parametro_sao_conferidos_contra_o_modelo() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    let modelo = id_do_modelo(&s);
    let b = base(&s, &rec);
    let tenta = |s: &mut PlanejamentoSqlite, v: ValorDeParametro| {
        s.aplicar(
            Alteracao::AplicarModelo {
                recorrente: rec.clone(),
                modelo: modelo.clone(),
                valores: vec![v],
                versao_base: b,
            },
            Autor::Mcp,
        )
    };
    for ruim in [
        valor("naoexiste", comp(2026, 1), 1),
        valor("dia_da_semana", comp(2026, 1), 0),
        valor("dia_da_semana", comp(2026, 1), 8),
        valor("diaria", comp(2026, 1), -1),
        ValorDeParametro {
            parametro: "diaria".into(),
            desde: comp(2026, 1),
            valor: ValorDoParametro::Texto("cem".into()),
        },
    ] {
        assert!(
            matches!(tenta(&mut s, ruim), Err(ErroDoPlanejamento::Invalida(_))),
            "deveria recusar"
        );
    }
    // Nada ficou ligado.
    assert_eq!(s.ligacao_de_modelo(&rec).expect("l"), None);
    // Definir parâmetro sem ligação também é recusado.
    let r = s.aplicar(
        Alteracao::DefinirParametro {
            recorrente: rec.clone(),
            valor: valor("diaria", comp(2026, 1), 1),
            versao_base: b,
        },
        Autor::Mcp,
    );
    assert!(matches!(r, Err(ErroDoPlanejamento::Invalida(_))));
}

#[test]
fn versao_base_velha_e_recusada() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    let modelo = id_do_modelo(&s);
    let r = s.aplicar(
        Alteracao::AplicarModelo {
            recorrente: rec,
            modelo,
            valores: vec![],
            versao_base: 99,
        },
        Autor::Mcp,
    );
    assert!(matches!(r, Err(ErroDoPlanejamento::VersaoVelha { .. })));
}

#[test]
fn compromisso_sem_modelo_projeta_360_meses_como_antes_e_sem_chamar_o_script() {
    let (mut s, ex) = servico();
    let sem = criar(&mut s, "Sem Modelo Exemplo", Autor::Usuario);
    let com = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &com, Autor::Usuario);
    let antes = ex.lotes();
    let o = s
        .proximas(&sem, comp(2026, 1), 360, HOJE)
        .expect("360 meses");
    assert_eq!(o.len(), 360);
    assert!(o.iter().all(|x| x.previsto == Some(Centavos(100_000))));
    assert!(o.iter().all(|x| x.calculo.is_none()));
    assert_eq!(o[359].id.competencia, comp(2055, 12));
    assert_eq!(ex.lotes(), antes, "sem modelo, o script não é chamado");
    // O calendário de 360 meses: só o do compromisso sem modelo é constante.
    let cal = s.calendario(comp(2026, 1), 360, HOJE).expect("calendário");
    assert_eq!(cal.len(), 360);
}

#[test]
fn projeta_360_meses_de_um_compromisso_com_modelo_num_lote_so() {
    let (mut s, ex) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &rec, Autor::Usuario);
    let lotes = ex.lotes();
    let o = s
        .proximas(&rec, comp(2026, 1), 360, HOJE)
        .expect("360 meses");
    assert_eq!(o.len(), 360);
    assert_eq!(ex.lotes() - lotes, 1, "um pedido com todas as competências");
    assert_eq!(ex.meses(), 360);
    assert!(
        o.iter()
            .all(|x| x.calculo.as_ref().is_some_and(|k| k.aviso.is_none()))
    );
}

#[test]
fn o_cache_nao_recalcula_sem_mudanca_e_so_refaz_o_que_mudou() {
    let (mut s, ex) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &rec, Autor::Usuario);
    let _ = s.proximas(&rec, comp(2026, 1), 12, HOJE).expect("1");
    let (lotes, meses) = (ex.lotes(), ex.meses());
    assert_eq!(meses, 12);
    // Projetar de novo (e o controle, e o calendário) não vai ao script.
    let _ = s.proximas(&rec, comp(2026, 1), 12, HOJE).expect("2");
    let _ = s.calendario(comp(2026, 1), 12, HOJE).expect("cal");
    let _ = s
        .controle(
            &sbf_planejamento::Periodo::MesCivil(comp(2026, 3)),
            sbf_planejamento::Filtro::Todos,
            None,
            HOJE,
        )
        .expect("controle");
    assert_eq!((ex.lotes(), ex.meses()), (lotes, meses));
    // Outro compromisso com os mesmos valores aproveita o cache (a chave não tem o id).
    let outro = criar(&mut s, "Outra Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &outro, Autor::Usuario);
    let _ = s.proximas(&outro, comp(2026, 1), 12, HOJE).expect("3");
    assert_eq!(ex.meses(), meses);
    // Mudar a diária de 2026-08 em diante: só os meses de 2026-08 a 2026-12 voltam ao script.
    let b = base(&s, &rec);
    s.aplicar(
        Alteracao::DefinirParametro {
            recorrente: rec.clone(),
            valor: valor("diaria", comp(2026, 8), 25_000),
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("parâmetro");
    let antes = ex.meses();
    let o = s.proximas(&rec, comp(2026, 1), 12, HOJE).expect("4");
    assert_eq!(ex.meses() - antes, 5);
    assert_eq!(o[7].previsto, Some(Centavos(4 * 25_000))); // agosto/2026: 4 sextas
    // Ajustar a quantidade de um mês refaz só aquele mês.
    let antes = ex.meses();
    s.aplicar(
        Alteracao::Excecao {
            ocorrencia: IdOcorrencia {
                recorrente: rec.clone(),
                competencia: comp(2026, 2),
                parcela: None,
            },
            excecao: Excecao::QuantidadeSoDesteMes { quantidade: 2 },
        },
        Autor::Usuario,
    )
    .expect("ajuste");
    let _ = s.proximas(&rec, comp(2026, 1), 12, HOJE).expect("5");
    assert_eq!(ex.meses() - antes, 1);
}

#[test]
fn script_com_erro_num_mes_cai_no_ultimo_valor_conhecido_e_nao_derruba_a_projecao() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Quebra Exemplo", Autor::Usuario);
    let calculo = "function calcular(e, a)
        if e.competencia == '2026-05' then error('quebrou') end
        return {valor_centavos = e.parametros.valor, explicacao = 'fixo'}
    end";
    let modelo = s
        .aplicar(
            Alteracao::CriarModelo {
                nome: "Quebradiço".into(),
                parametros: vec![ParametroDeModelo {
                    nome: "valor".into(),
                    tipo: TipoDeParametro::Centavos,
                }],
                calculo: calculo.into(),
            },
            Autor::Usuario,
        )
        .expect("modelo")
        .alvo;
    let b = base(&s, &rec);
    s.aplicar(
        Alteracao::AplicarModelo {
            recorrente: rec.clone(),
            modelo,
            valores: vec![valor("valor", comp(2026, 1), 33_300)],
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("aplicar");
    let o = s.proximas(&rec, comp(2026, 3), 5, HOJE).expect("projeção");
    assert_eq!(o.len(), 5);
    let em = |c| o.iter().find(|x| x.id.competencia == c).expect("mês");
    assert_eq!(em(comp(2026, 4)).previsto, Some(Centavos(33_300)));
    // Maio: o script quebrou, vale o último valor conhecido (abril), com o motivo.
    let maio = em(comp(2026, 5));
    assert_eq!(maio.previsto, Some(Centavos(33_300)));
    let aviso = maio
        .calculo
        .as_ref()
        .and_then(|k| k.aviso.as_deref())
        .expect("aviso");
    assert!(aviso.contains("quebrou"), "{aviso}");
    assert_eq!(
        em(comp(2026, 6))
            .calculo
            .as_ref()
            .and_then(|k| k.aviso.clone()),
        None
    );
    // E o controle, o calendário e a simulação seguem funcionando.
    s.controle(
        &sbf_planejamento::Periodo::MesCivil(comp(2026, 5)),
        sbf_planejamento::Filtro::Todos,
        None,
        HOJE,
    )
    .expect("controle");
    s.calendario(comp(2026, 1), 12, HOJE).expect("calendário");
}

#[test]
fn script_que_falha_logo_no_primeiro_mes_cai_no_valor_da_versao() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Quebra Exemplo", Autor::Usuario);
    let modelo = s
        .aplicar(
            Alteracao::CriarModelo {
                nome: "Sempre quebra".into(),
                parametros: vec![ParametroDeModelo {
                    nome: "valor".into(),
                    tipo: TipoDeParametro::Centavos,
                }],
                calculo: "function calcular(e, a) error('nunca') end".into(),
            },
            Autor::Usuario,
        )
        .expect("modelo")
        .alvo;
    let b = base(&s, &rec);
    s.aplicar(
        Alteracao::AplicarModelo {
            recorrente: rec.clone(),
            modelo,
            valores: vec![valor("valor", comp(2026, 1), 1)],
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("aplicar");
    let o = mes(&s, &rec, comp(2026, 3));
    assert_eq!(o.previsto, Some(Centavos(100_000)));
    assert!(o.calculo.expect("c").aviso.is_some());
}

#[test]
fn sem_executor_o_compromisso_cai_no_valor_da_versao_com_aviso() {
    // Um serviço sem `com_calculo`: o modelo existe, mas ninguém roda o script.
    let mut s = PlanejamentoSqlite::novo(Banco::em_memoria().expect("banco"), Box::new(SemSaldos));
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &rec, Autor::Usuario);
    let o = mes(&s, &rec, comp(2026, 5));
    assert_eq!(o.previsto, Some(Centavos(100_000)));
    assert!(o.calculo.expect("c").aviso.is_some());
}

#[test]
fn parametro_faltando_num_mes_avisa_sem_derrubar() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    let modelo = id_do_modelo(&s);
    let b = base(&s, &rec);
    // Só a diária: falta o dia da semana.
    s.aplicar(
        Alteracao::AplicarModelo {
            recorrente: rec.clone(),
            modelo,
            valores: vec![valor("diaria", comp(2026, 1), 19_000)],
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("aplicar");
    let o = mes(&s, &rec, comp(2026, 3));
    assert_eq!(o.previsto, Some(Centavos(100_000)));
    let aviso = o.calculo.expect("c").aviso.expect("aviso");
    assert!(aviso.contains("dia_da_semana"), "{aviso}");
}

#[test]
fn nova_versao_do_calculo_nao_muda_quem_ja_esta_ligado_ate_ligar_de_novo() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &rec, Autor::Usuario);
    let modelo = id_do_modelo(&s);
    let nova = "function calcular(e, a) return {valor_centavos = 1, explicacao = 'v2'} end";
    let r = s
        .aplicar(
            Alteracao::NovaVersaoDoCalculo {
                modelo: modelo.clone(),
                calculo: nova.into(),
            },
            Autor::Usuario,
        )
        .expect("nova versão");
    let m = s
        .modelos()
        .expect("m")
        .into_iter()
        .find(|m| m.id == modelo)
        .expect("modelo");
    assert_eq!(m.versao_do_calculo, 2);
    // Quem estava ligado continua na versão 1.
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(105_000));
    assert_eq!(
        s.ligacao_de_modelo(&rec)
            .expect("l")
            .expect("há")
            .versao_do_calculo,
        1
    );
    // Ligar de novo (sem valores novos) passa para a versão 2.
    let b = base(&s, &rec);
    s.aplicar(
        Alteracao::AplicarModelo {
            recorrente: rec.clone(),
            modelo: modelo.clone(),
            valores: vec![],
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("religar");
    assert_eq!(previsto(&s, &rec, comp(2026, 5)), Some(1));
    // Repetir o mesmo script é recusado; sem `calcular` também.
    for ruim in [nova, "x = 1"] {
        let r = s.aplicar(
            Alteracao::NovaVersaoDoCalculo {
                modelo: modelo.clone(),
                calculo: ruim.into(),
            },
            Autor::Usuario,
        );
        assert!(matches!(r, Err(ErroDoPlanejamento::Invalida(_))));
    }
    // A versão 2 está em uso: não dá para desfazer a criação dela.
    assert!(s.desfazer(&r.alteracao_id, Autor::Usuario).is_err());
}

#[test]
fn o_banco_nao_deixa_editar_nem_apagar_o_calculo() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    ligar_a_diarista(&mut s, &rec, Autor::Usuario);
    let c = s.banco.conexao();
    assert!(c.execute("UPDATE calculos SET fonte = 'x'", []).is_err());
    assert!(c.execute("DELETE FROM calculos", []).is_err());
    assert!(c.execute("DELETE FROM modelos", []).is_err());
    assert!(c.execute("DELETE FROM ligacoes_de_modelo", []).is_err());
    assert!(c.execute("DELETE FROM valores_de_parametro", []).is_err());
    assert_eq!(s.banco.versao_do_esquema().expect("v"), 9);
}

#[test]
fn simular_aplicar_modelo_mostra_o_efeito_sem_gravar() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    let modelo = id_do_modelo(&s);
    let b = base(&s, &rec);
    let sim = s
        .simular(
            &Alteracao::AplicarModelo {
                recorrente: rec.clone(),
                modelo,
                valores: vec![
                    valor("diaria", comp(2026, 1), 19_000),
                    valor("dia_da_semana", comp(2026, 1), 5),
                ],
                versao_base: b,
            },
            HOJE,
        )
        .expect("simular");
    assert!(sim.mudancas.iter().any(|(c, antes, depois)| {
        *c == comp(2026, 4)
            && *antes == Some(Centavos(100_000))
            && *depois == Some(Centavos(76_000))
    }));
    assert_eq!(s.ligacao_de_modelo(&rec).expect("l"), None);
    assert_eq!(previsto(&s, &rec, comp(2026, 4)), Some(100_000));
}

#[test]
fn o_historico_registra_quem_aplicou_e_o_mcp_so_desfaz_o_proprio() {
    let (mut s, _) = servico();
    let rec = criar(&mut s, "Diarista Exemplo", Autor::Usuario);
    let id = ligar_a_diarista(&mut s, &rec, Autor::Mcp);
    let h = s.entrada_do_historico(&id).expect("h").expect("há");
    assert_eq!(h.autor, Autor::Mcp);
    assert!(h.resumo.contains(FABRICA_NOME_DO_MODELO));
    assert_eq!(
        s.ultima_desfazivel(&Autor::Mcp).expect("u").map(|e| e.id),
        Some(id)
    );
}
