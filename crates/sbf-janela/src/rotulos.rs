//! O **rótulo da conta** na janela: a leitura passa por aqui e toda conta do extrato com par
//! confirmado aparece como `CX-0001 · Apelido` (a poupança vinculada, como `Poupança CX-0001 ·
//! Apelido`). O inventário não conhece o planejamento; quem junta os dois é este
//! [`ConsultaRotulada`], que embrulha a [`Consulta`] e troca o **texto** dos links de conta.
//! A identidade (`Ref { Conta, chave }`) continua sendo a conta do extrato, e o tooltip do link
//! a mostra (ver `componentes::link_item`).
//!
//! A pesquisa também aceita o rótulo: o texto do filtro que casa com o rótulo de uma conta
//! leva essa conta para `Filtro::contas_do_texto`, e o SQL a trata como mais uma alternativa.
//! Nenhuma conta some de nenhuma lista: o rótulo só troca o nome.

use std::sync::{Arc, RwLock};

use sbf_inventario::consulta::{
    Area, Campo, Celula, Consulta, Contagens, Elo, Filtro, Fonte, Item, PaginaDeTabela, Painel,
    PainelDeArea, PedidoDeTabela, Pergunta, Rastro, ResultadoDeConsulta, Secao, Tabela,
    TotalDoCiclo,
};
use sbf_inventario::{Ref, Tipo};
use sbf_planejamento::Rotulador;

/// O rótulo de agora, compartilhado entre a janela (que o atualiza quando o planejamento
/// muda) e a [`ConsultaRotulada`] (que o aplica). Barato de clonar.
#[derive(Debug, Clone, Default)]
pub struct RotulosDasContas(Arc<RwLock<Rotulador>>);

impl RotulosDasContas {
    /// Troca o rótulo (depois de ler o planejamento de novo).
    pub fn definir(&self, rotulador: Rotulador) {
        if let Ok(mut r) = self.0.write() {
            *r = rotulador;
        }
    }

    /// Uma cópia do rótulo de agora.
    #[must_use]
    pub fn atual(&self) -> Rotulador {
        self.0.read().map(|r| r.clone()).unwrap_or_default()
    }
}

/// A [`Consulta`] com os nomes das contas trocados pelo rótulo.
pub struct ConsultaRotulada {
    base: Box<dyn Consulta>,
    rotulos: RotulosDasContas,
}

impl std::fmt::Debug for ConsultaRotulada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsultaRotulada").finish_non_exhaustive()
    }
}

impl ConsultaRotulada {
    #[must_use]
    pub fn nova(base: Box<dyn Consulta>, rotulos: RotulosDasContas) -> Self {
        Self { base, rotulos }
    }
}

/// O texto de um link de conta, trocado pelo rótulo quando a conta tem par.
fn texto_do_link(r: &Rotulador, alvo: &Ref, texto: &str) -> String {
    if alvo.tipo == Tipo::Conta {
        r.rotulo(&alvo.chave).unwrap_or_else(|| texto.to_owned())
    } else {
        texto.to_owned()
    }
}

fn celula(r: &Rotulador, c: Celula) -> Celula {
    match c {
        Celula::Link { alvo, texto } => {
            let texto = texto_do_link(r, &alvo, &texto);
            Celula::Link { alvo, texto }
        }
        Celula::Links(lista) => Celula::Links(
            lista
                .into_iter()
                .map(|(alvo, texto)| {
                    let texto = texto_do_link(r, &alvo, &texto);
                    (alvo, texto)
                })
                .collect(),
        ),
        outra => outra,
    }
}

fn tabela(r: &Rotulador, mut t: Tabela) -> Tabela {
    for linha in &mut t.linhas {
        for c in &mut linha.celulas {
            *c = celula(r, std::mem::replace(c, Celula::Vazia));
        }
    }
    t
}

fn painel(r: &Rotulador, mut p: Painel) -> Painel {
    p.secoes = secoes(r, p.secoes);
    p
}

fn secoes(r: &Rotulador, s: Vec<Secao>) -> Vec<Secao> {
    s.into_iter()
        .map(|mut s| {
            s.tabela = tabela(r, s.tabela);
            s
        })
        .collect()
}

fn item(r: &Rotulador, mut i: Item) -> Item {
    if i.alvo.tipo == Tipo::Conta
        && let Some(rotulo) = r.rotulo(&i.alvo.chave)
    {
        i.rotulo = rotulo;
    }
    i.campos = i
        .campos
        .into_iter()
        .map(|Campo { rotulo, valor }| Campo {
            rotulo,
            valor: celula(r, valor),
        })
        .collect();
    i
}

fn elo(r: &Rotulador, mut e: Elo) -> Elo {
    if e.alvo.tipo == Tipo::Conta
        && let Some(rotulo) = r.rotulo(&e.alvo.chave)
    {
        e.rotulo = rotulo;
    }
    e
}

/// O filtro com as contas que o texto acha pelo rótulo. Só as que têm rótulo: as outras o
/// SQL já acha pelo nome do banco.
fn filtro(r: &Rotulador, f: &Filtro) -> Filtro {
    let mut f = f.clone();
    if let Some(t) = f.texto.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        f.contas_do_texto = r
            .contas_que_casam(t)
            .into_iter()
            .filter(|c| r.rotulo(c).is_some())
            .collect();
    }
    f
}

fn pedido(r: &Rotulador, p: &PedidoDeTabela) -> PedidoDeTabela {
    match p {
        PedidoDeTabela::Area { area, filtro: f } => PedidoDeTabela::Area {
            area: *area,
            filtro: filtro(r, f),
        },
        outro => outro.clone(),
    }
}

impl Consulta for ConsultaRotulada {
    fn tabela(&self, p: &PedidoDeTabela) -> ResultadoDeConsulta<Tabela> {
        let r = self.rotulos.atual();
        self.base.tabela(&pedido(&r, p)).map(|t| tabela(&r, t))
    }

    fn tabela_paginada(
        &self,
        p: &PedidoDeTabela,
        inicio: usize,
        limite: usize,
    ) -> ResultadoDeConsulta<PaginaDeTabela> {
        let r = self.rotulos.atual();
        self.base
            .tabela_paginada(&pedido(&r, p), inicio, limite)
            .map(|mut pg| {
                pg.tabela = tabela(&r, pg.tabela);
                pg
            })
    }

    fn item(&self, alvo: &Ref) -> ResultadoDeConsulta<Option<Item>> {
        let r = self.rotulos.atual();
        self.base.item(alvo).map(|i| i.map(|i| item(&r, i)))
    }

    fn rastro(&self, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
        let r = self.rotulos.atual();
        self.base.rastro(alvo).map(|ra| Rastro {
            acima: ra.acima.into_iter().map(|e| elo(&r, e)).collect(),
            abaixo: ra.abaixo.into_iter().map(|e| elo(&r, e)).collect(),
        })
    }

    fn por_ciclo(
        &self,
        alvo: &Ref,
        fonte: Fonte,
        ciclos: usize,
    ) -> ResultadoDeConsulta<Vec<TotalDoCiclo>> {
        self.base.por_ciclo(alvo, fonte, ciclos)
    }

    fn visao_geral(&self) -> ResultadoDeConsulta<Painel> {
        let r = self.rotulos.atual();
        self.base.visao_geral().map(|p| painel(&r, p))
    }

    fn relatorio(
        &self,
        pergunta: Pergunta,
        fonte: Fonte,
        f: &Filtro,
    ) -> ResultadoDeConsulta<Painel> {
        let r = self.rotulos.atual();
        self.base
            .relatorio(pergunta, fonte, &filtro(&r, f))
            .map(|p| painel(&r, p))
    }

    fn contagens(&self) -> ResultadoDeConsulta<Contagens> {
        self.base.contagens()
    }

    fn painel_da_area(&self, area: Area, f: &Filtro) -> ResultadoDeConsulta<PainelDeArea> {
        let r = self.rotulos.atual();
        self.base.painel_da_area(area, &filtro(&r, f)).map(|mut p| {
            p.secoes = secoes(&r, p.secoes);
            for i in &mut p.indicadores {
                i.valor = celula(&r, std::mem::replace(&mut i.valor, Celula::Vazia));
            }
            p
        })
    }

    fn texto(&self, achado: &Ref) -> ResultadoDeConsulta<Option<String>> {
        self.base.texto(achado)
    }
}

#[cfg(test)]
mod testes {
    use std::cell::RefCell;
    use std::rc::Rc;

    use sbf_inventario::consulta::{Coluna, ErroDeConsulta, Linha};
    use sbf_planejamento::ContaDoPlanejamento;

    use super::*;

    const COM_PAR: &str = "BANCO EXEMPLO · 000000-1";
    const POUPANCA: &str = "BANCO EXEMPLO Poupança · 000000-1";
    const SEM_PAR: &str = "OUTRO BANCO · 555555-5";

    /// Uma leitura que devolve uma linha com links de conta e guarda o último pedido.
    #[derive(Default)]
    struct Falsa {
        ultimo: Rc<RefCell<Option<PedidoDeTabela>>>,
    }

    type Ultimo = Rc<RefCell<Option<PedidoDeTabela>>>;

    fn link(conta: &str) -> Celula {
        Celula::Link {
            alvo: Ref::nova(Tipo::Conta, conta),
            texto: conta.to_owned(),
        }
    }

    impl Consulta for Falsa {
        fn tabela(&self, p: &PedidoDeTabela) -> ResultadoDeConsulta<Tabela> {
            *self.ultimo.borrow_mut() = Some(p.clone());
            Ok(Tabela {
                colunas: vec![Coluna {
                    rotulo: "Conta".into(),
                    numerica: false,
                }],
                linhas: [COM_PAR, POUPANCA, SEM_PAR]
                    .iter()
                    .map(|c| Linha {
                        alvo: None,
                        celulas: vec![
                            link(c),
                            Celula::Link {
                                alvo: Ref::nova(Tipo::Fornecedor, "energia"),
                                texto: "Energia".into(),
                            },
                        ],
                    })
                    .collect(),
            })
        }
        fn item(&self, alvo: &Ref) -> ResultadoDeConsulta<Option<Item>> {
            Ok(Some(Item {
                alvo: alvo.clone(),
                rotulo: alvo.chave.clone(),
                campos: vec![Campo {
                    rotulo: "Conta".into(),
                    valor: link(COM_PAR),
                }],
            }))
        }
        fn rastro(&self, _: &Ref) -> ResultadoDeConsulta<Rastro> {
            Ok(Rastro {
                acima: vec![Elo {
                    alvo: Ref::nova(Tipo::Conta, COM_PAR),
                    rotulo: COM_PAR.into(),
                    papel: "na conta".into(),
                    quem: None,
                    quando: None,
                }],
                abaixo: Vec::new(),
            })
        }
        fn por_ciclo(&self, _: &Ref, _: Fonte, _: usize) -> ResultadoDeConsulta<Vec<TotalDoCiclo>> {
            Ok(Vec::new())
        }
        fn visao_geral(&self) -> ResultadoDeConsulta<Painel> {
            Err(ErroDeConsulta::NaoSuportado("x".into()))
        }
        fn relatorio(&self, _: Pergunta, _: Fonte, _: &Filtro) -> ResultadoDeConsulta<Painel> {
            Err(ErroDeConsulta::NaoSuportado("x".into()))
        }
        fn contagens(&self) -> ResultadoDeConsulta<Contagens> {
            Ok(Contagens::default())
        }
        fn texto(&self, _: &Ref) -> ResultadoDeConsulta<Option<String>> {
            Ok(None)
        }
    }

    fn rotulada() -> (ConsultaRotulada, RotulosDasContas, Ultimo) {
        let ultimo = Ultimo::default();
        let rotulos = RotulosDasContas::default();
        rotulos.definir(Rotulador::novo(
            &[ContaDoPlanejamento {
                codigo: "CX-0001".into(),
                apelido: Some("Reserva".into()),
                conta_do_extrato: Some(COM_PAR.into()),
            }],
            &[COM_PAR.into(), POUPANCA.into(), SEM_PAR.into()],
        ));
        let falsa = Falsa {
            ultimo: Rc::clone(&ultimo),
        };
        (
            ConsultaRotulada::nova(Box::new(falsa), rotulos.clone()),
            rotulos,
            ultimo,
        )
    }

    fn texto_de(c: &Celula) -> &str {
        match c {
            Celula::Link { texto, .. } => texto,
            _ => panic!("não é link"),
        }
    }

    #[test]
    fn links_de_conta_viram_rotulo_e_o_resto_fica() {
        let (c, _, _) = rotulada();
        let t = c
            .tabela(&PedidoDeTabela::Area {
                area: Area::Registros,
                filtro: Filtro::default(),
            })
            .unwrap();
        let contas: Vec<&str> = t.linhas.iter().map(|l| texto_de(&l.celulas[0])).collect();
        assert_eq!(
            contas,
            vec!["CX-0001 · Reserva", "Poupança CX-0001 · Reserva", SEM_PAR]
        );
        assert_eq!(texto_de(&t.linhas[0].celulas[1]), "Energia");
        // Nenhuma linha some: a identidade continua a conta do extrato.
        assert_eq!(t.linhas.len(), 3);
        let Celula::Link { alvo, .. } = &t.linhas[1].celulas[0] else {
            panic!("link");
        };
        assert_eq!(alvo.chave, POUPANCA);
    }

    #[test]
    fn pagina_do_item_e_rastro_mostram_o_rotulo() {
        let (c, _, _) = rotulada();
        let i = c
            .item(&Ref::nova(Tipo::Conta, COM_PAR))
            .unwrap()
            .expect("item");
        assert_eq!(i.rotulo, "CX-0001 · Reserva");
        assert_eq!(texto_de(&i.campos[0].valor), "CX-0001 · Reserva");
        let sem = c
            .item(&Ref::nova(Tipo::Conta, SEM_PAR))
            .unwrap()
            .expect("item");
        assert_eq!(sem.rotulo, SEM_PAR);
        let r = c.rastro(&Ref::nova(Tipo::Registro, "1")).unwrap();
        assert_eq!(r.acima[0].rotulo, "CX-0001 · Reserva");
    }

    #[test]
    fn a_pesquisa_pelo_rotulo_leva_as_contas_para_o_filtro() {
        let (c, _, ultimo) = rotulada();
        let pedir = |texto: &str| {
            c.tabela(&PedidoDeTabela::Area {
                area: Area::Registros,
                filtro: Filtro {
                    texto: Some(texto.into()),
                    ..Filtro::default()
                },
            })
            .unwrap();
            let ultimo = ultimo.borrow().clone();
            match ultimo {
                Some(PedidoDeTabela::Area { filtro, .. }) => filtro,
                outro => panic!("pedido inesperado: {outro:?}"),
            }
        };
        let f = pedir("reserva");
        assert_eq!(f.texto.as_deref(), Some("reserva"));
        let mut contas = f.contas_do_texto;
        contas.sort();
        assert_eq!(contas, vec![POUPANCA.to_owned(), COM_PAR.to_owned()]);
        // O nome do banco acha as contas pelo SQL; só as com rótulo precisam de ajuda.
        let f = pedir("banco exemplo");
        assert!(
            f.contas_do_texto
                .iter()
                .all(|c| c.contains("BANCO EXEMPLO"))
        );
        assert!(!f.contas_do_texto.contains(&SEM_PAR.to_owned()));
        assert!(pedir("zzz").contas_do_texto.is_empty());
    }

    #[test]
    fn mudar_o_rotulo_vale_para_a_proxima_leitura() {
        let (c, rotulos, _) = rotulada();
        rotulos.definir(Rotulador::default());
        let t = c
            .tabela(&PedidoDeTabela::Area {
                area: Area::Registros,
                filtro: Filtro::default(),
            })
            .unwrap();
        assert_eq!(texto_de(&t.linhas[0].celulas[0]), COM_PAR);
    }
}
