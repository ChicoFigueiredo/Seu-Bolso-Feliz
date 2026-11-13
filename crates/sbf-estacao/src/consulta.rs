//! A `Consulta` do livro-caixa (`Fonte::LivroCaixa`) sobre o cache (spec 07, T9): as tabelas
//! das áreas de Dinheiro, as páginas de item, os totais clicáveis, o rastro e o
//! `painel_da_area`, com os números do `sbf-livro` (uma regra, um código: ADR-012).
//!
//! A cópia é lida inteira para a memória ([`ConsultaDoLivro::do_cache`]); a janela a relê
//! quando a estação termina uma troca. Nenhum número daqui leva "a conferir".

use crate::cache::{CacheDoLivro, ErroDoCache};
use crate::texto::dia_e_mes;
use sbf_contrato::{Colecao, LinhasDaColecao};
use sbf_inventario::consulta::{
    Area, Campo, Celula, Coluna, Consulta, Contagens, Elo, ErroDeConsulta, Filtro, Indicador, Item,
    Linha, Painel, PainelDeArea, PedidoDeTabela, Pergunta, Rastro, ResultadoDeConsulta, Secao,
    Tabela, TotalDoCiclo,
};
use sbf_inventario::referencia::{Ref, Tipo};
use sbf_livro::centros::painel_centros;
use sbf_livro::contrato::livro_das_linhas;
use sbf_livro::gastos::GastosDoLivro;
use sbf_livro::hoje::{MotivoParaResolver, PainelHoje, e_entrada, e_saida, painel_hoje};
use sbf_livro::{Ciclo, Data, LivroCaixa, Movimentacao, TipoDeMovimentacao};

impl From<ErroDoCache> for ErroDeConsulta {
    fn from(e: ErroDoCache) -> Self {
        ErroDeConsulta::Banco(e.0)
    }
}

/// Recorte de [`PedidoDeTabela::ItensDoTotal`] para os números deste módulo.
const RECORTE_DISPONIVEL: &str = "disponivel";
const RECORTE_ENTROU: &str = "entrou";
const RECORTE_SAIU: &str = "saiu";
const RECORTE_NO_CARTAO: &str = "no-cartao";

/// O recorte de cada item de Para resolver (as convenções da janela).
fn recorte_de(motivo: MotivoParaResolver) -> &'static str {
    match motivo {
        MotivoParaResolver::Vencidas => "vencidos",
        MotivoParaResolver::SemCategoria => "sem-categoria",
        MotivoParaResolver::NaoAlocadas => "nao-alocadas",
        MotivoParaResolver::NaoConfirmadas => "nao-confirmadas",
        MotivoParaResolver::DocumentosARevisar => "documentos-a-revisar",
    }
}

fn linhas_do_cache(cache: &CacheDoLivro, colecao: Colecao) -> Result<LinhasDaColecao, ErroDoCache> {
    use LinhasDaColecao as L;
    Ok(match colecao {
        Colecao::Contas => L::Contas(cache.linhas(colecao)?),
        Colecao::Instituicoes => L::Instituicoes(cache.linhas(colecao)?),
        Colecao::Cartoes => L::Cartoes(cache.linhas(colecao)?),
        Colecao::Faturas => L::Faturas(cache.linhas(colecao)?),
        Colecao::ItensDeFatura => L::ItensDeFatura(cache.linhas(colecao)?),
        Colecao::Movimentacoes => L::Movimentacoes(cache.linhas(colecao)?),
        Colecao::Transferencias => L::Transferencias(cache.linhas(colecao)?),
        Colecao::Rateios => L::Rateios(cache.linhas(colecao)?),
        Colecao::Categorias => L::Categorias(cache.linhas(colecao)?),
        Colecao::Tags => L::Tags(cache.linhas(colecao)?),
        Colecao::Centros => L::Centros(cache.linhas(colecao)?),
        Colecao::Fornecedores => L::Fornecedores(cache.linhas(colecao)?),
        Colecao::Apelidos => L::Apelidos(cache.linhas(colecao)?),
        Colecao::Contratos => L::Contratos(cache.linhas(colecao)?),
        Colecao::Dividas => L::Dividas(cache.linhas(colecao)?),
        Colecao::Parcelas => L::Parcelas(cache.linhas(colecao)?),
        Colecao::Recorrentes => L::Recorrentes(cache.linhas(colecao)?),
        Colecao::Ocorrencias => L::Ocorrencias(cache.linhas(colecao)?),
        Colecao::Documentos => L::Documentos(cache.linhas(colecao)?),
    })
}

/// O livro-caixa da cópia local, pronto para as telas.
#[derive(Debug, Clone)]
pub struct ConsultaDoLivro {
    livro: LivroCaixa,
    copia_de: Option<String>,
    hoje: Data,
}

impl ConsultaDoLivro {
    /// Lê a cópia inteira do cache. Sem `/estado` ainda (nunca sincronizou), o livro vem
    /// vazio com o corte padrão.
    ///
    /// # Errors
    /// Falha do cache ou linha que não converte.
    pub fn do_cache(cache: &CacheDoLivro) -> ResultadoDeConsulta<Self> {
        let estado = cache.estado_do_servidor()?;
        let mut colecoes = Vec::with_capacity(Colecao::TODAS.len());
        for c in Colecao::TODAS {
            colecoes.push(linhas_do_cache(cache, c)?);
        }
        let corte = estado.as_ref().map_or(20, |e| e.dia_de_corte);
        let mut livro = livro_das_linhas(corte, &colecoes)
            .map_err(|e| ErroDeConsulta::Banco(format!("livro-caixa: {e}")))?;
        livro.documentos_a_revisar = estado.as_ref().map_or(0, |e| e.documentos_a_revisar);
        Ok(Self {
            livro,
            copia_de: cache.copia_de()?,
            hoje: Data::hoje_em_sao_paulo(),
        })
    }

    /// Fixa o dia de "hoje" (testes).
    pub fn com_hoje(mut self, hoje: Data) -> Self {
        self.hoje = hoje;
        self
    }

    pub fn livro(&self) -> &LivroCaixa {
        &self.livro
    }

    /// O instante (RFC 3339) da cópia.
    pub fn copia_de(&self) -> Option<&str> {
        self.copia_de.as_deref()
    }

    /// A `FonteDeGastos` dos Relatórios (`responder_com`, Q18).
    pub fn gastos(&self) -> GastosDoLivro<'_> {
        GastosDoLivro::novo(&self.livro)
    }

    fn ciclo(&self) -> Ciclo {
        Ciclo::da_data(self.hoje, self.livro.corte)
    }

    fn nome_de<'a>(&'a self, tipo: Tipo, id: &str) -> Option<&'a str> {
        let l = &self.livro;
        match tipo {
            Tipo::Conta => l
                .contas
                .iter()
                .find(|x| x.id == id)
                .map(|x| x.nome.as_str()),
            Tipo::Categoria => l
                .categorias
                .iter()
                .find(|x| x.id == id)
                .map(|x| x.nome.as_str()),
            Tipo::Centro => l
                .centros
                .iter()
                .find(|x| x.id == id)
                .map(|x| x.nome.as_str()),
            Tipo::Fornecedor => l
                .fornecedores
                .iter()
                .find(|x| x.id == id)
                .map(|x| x.nome.as_str()),
            Tipo::Divida => l
                .dividas
                .iter()
                .find(|x| x.id == id)
                .map(|x| x.nome.as_str()),
            Tipo::Recorrente => l
                .recorrentes
                .iter()
                .find(|x| x.id == id)
                .map(|x| x.nome.as_str()),
            Tipo::Instituicao => l
                .instituicoes
                .iter()
                .find(|x| x.id == id)
                .map(|x| x.nome.as_str()),
            _ => None,
        }
    }

    fn link(&self, tipo: Tipo, id: Option<&str>) -> Celula {
        match id {
            Some(id) => Celula::Link {
                alvo: Ref::nova(tipo, id),
                texto: self.nome_de(tipo, id).unwrap_or("(sem nome)").to_owned(),
            },
            None => Celula::Vazia,
        }
    }

    fn valor(&self, centavos: i64, itens: Option<PedidoDeTabela>) -> Celula {
        Celula::Valor {
            centavos,
            fonte: sbf_inventario::consulta::Fonte::LivroCaixa,
            itens: itens.map(Box::new),
        }
    }

    fn itens(&self, de: Ref, ciclo: Option<&Ciclo>, recorte: &str) -> PedidoDeTabela {
        PedidoDeTabela::ItensDoTotal {
            de,
            fonte: sbf_inventario::consulta::Fonte::LivroCaixa,
            ciclo: ciclo.map(Ciclo::nome),
            recorte: Some(recorte.to_owned()),
        }
    }

    // ---------------- movimentações ----------------

    fn tabela_de_movimentacoes<'a>(
        &'a self,
        movs: impl Iterator<Item = &'a Movimentacao>,
    ) -> Tabela {
        let mut movs: Vec<&Movimentacao> = movs.collect();
        movs.sort_by(|a, b| b.data.cmp(&a.data).then_with(|| a.id.cmp(&b.id)));
        Tabela {
            colunas: colunas(&[
                ("Data", false),
                ("Descrição", false),
                ("Conta", false),
                ("Categoria", false),
                ("Valor", true),
                ("Confirmada", false),
            ]),
            linhas: movs
                .into_iter()
                .map(|m| Linha {
                    alvo: Some(Ref::nova(Tipo::Movimentacao, m.id.clone())),
                    celulas: vec![
                        Celula::Data(m.data.to_string()),
                        Celula::Link {
                            alvo: Ref::nova(Tipo::Movimentacao, m.id.clone()),
                            texto: descricao(m),
                        },
                        self.link(Tipo::Conta, Some(&m.conta_id)),
                        self.link(Tipo::Categoria, m.categoria_id.as_deref()),
                        self.valor(com_sinal(m), None),
                        Celula::Texto(if m.confirmada { "Sim" } else { "Não" }.to_owned()),
                    ],
                })
                .collect(),
        }
    }

    /// As movimentações ligadas a um item (todo total abre as que o somam).
    fn movimentacoes_de<'a>(
        &'a self,
        de: &'a Ref,
    ) -> Box<dyn Iterator<Item = &'a Movimentacao> + 'a> {
        let l = &self.livro;
        let id = de.chave.as_str();
        match de.tipo {
            Tipo::Conta => Box::new(l.movimentacoes.iter().filter(move |m| m.conta_id == id)),
            Tipo::Categoria => Box::new(
                l.movimentacoes
                    .iter()
                    .filter(move |m| m.categoria_id.as_deref() == Some(id)),
            ),
            Tipo::Fornecedor => Box::new(
                l.movimentacoes
                    .iter()
                    .filter(move |m| m.fornecedor_id.as_deref() == Some(id)),
            ),
            Tipo::Fatura => Box::new(
                l.movimentacoes
                    .iter()
                    .filter(move |m| m.fatura_id.as_deref() == Some(id)),
            ),
            Tipo::Centro => Box::new(l.movimentacoes.iter().filter(move |m| {
                l.rateios
                    .iter()
                    .any(|r| r.centro_id == id && r.movimentacao_id == m.id)
            })),
            Tipo::Divida => Box::new(l.movimentacoes.iter().filter(move |m| {
                m.parcela_id
                    .as_deref()
                    .is_some_and(|p| l.parcelas.iter().any(|x| x.id == p && x.divida_id == id))
            })),
            Tipo::Recorrente => Box::new(l.movimentacoes.iter().filter(move |m| {
                m.ocorrencia_id.as_deref().is_some_and(|o| {
                    l.ocorrencias
                        .iter()
                        .any(|x| x.id == o && x.recorrente_id == id)
                })
            })),
            Tipo::Ciclo => {
                let ciclo = Ciclo::do_nome(id, l.corte);
                Box::new(
                    l.movimentacoes
                        .iter()
                        .filter(move |m| ciclo.as_ref().is_some_and(|c| c.contem(m.data))),
                )
            }
            _ => Box::new(std::iter::empty()),
        }
    }

    fn filtrar<'a>(&'a self, filtro: &'a Filtro) -> impl Iterator<Item = &'a Movimentacao> + 'a {
        let texto = filtro.texto.as_ref().map(|t| t.to_lowercase());
        let ciclos = filtro.ciclos.clone();
        self.livro.movimentacoes.iter().filter(move |m| {
            let texto_ok = texto
                .as_ref()
                .is_none_or(|t| descricao(m).to_lowercase().contains(t));
            let ciclo_ok = ciclos.as_ref().is_none_or(|(de, ate)| {
                let nome = Ciclo::da_data(m.data, self.livro.corte).nome();
                &nome >= de && &nome <= ate
            });
            texto_ok && ciclo_ok
        })
    }

    // ---------------- tabelas das áreas ----------------

    fn tabela_da_area(&self, area: Area, filtro: &Filtro) -> ResultadoDeConsulta<Tabela> {
        let l = &self.livro;
        let ciclo = self.ciclo();
        Ok(match area {
            Area::Movimentacoes => self.tabela_de_movimentacoes(self.filtrar(filtro)),
            Area::ContasECartoes => Tabela {
                colunas: colunas(&[
                    ("Conta", false),
                    ("Instituição", false),
                    ("Tipo", false),
                    ("Saldo", true),
                ]),
                linhas: sbf_livro::hoje::contas_em_ordem(l)
                    .into_iter()
                    .map(|c| {
                        let alvo = Ref::nova(Tipo::Conta, c.id.clone());
                        Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                self.link(Tipo::Conta, Some(&c.id)),
                                self.link(Tipo::Instituicao, Some(&c.instituicao_id)),
                                Celula::Texto(c.tipo.codigo().to_owned()),
                                self.valor(
                                    c.saldo.0,
                                    Some(PedidoDeTabela::Ligados {
                                        de: alvo,
                                        tipo: Tipo::Movimentacao,
                                    }),
                                ),
                            ],
                        }
                    })
                    .collect(),
            },
            Area::Dividas => Tabela {
                colunas: colunas(&[
                    ("Dívida", false),
                    ("Saldo devedor", true),
                    ("Parcelas", false),
                    ("Situação", false),
                ]),
                linhas: l
                    .dividas
                    .iter()
                    .map(|d| {
                        let alvo = Ref::nova(Tipo::Divida, d.id.clone());
                        Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                self.link(Tipo::Divida, Some(&d.id)),
                                self.valor(
                                    d.saldo_devedor.0,
                                    Some(PedidoDeTabela::Ligados {
                                        de: alvo,
                                        tipo: Tipo::Movimentacao,
                                    }),
                                ),
                                Celula::Texto(match d.total_de_parcelas {
                                    Some(t) => format!("{} de {t}", d.parcelas_pagas),
                                    None => d.parcelas_pagas.to_string(),
                                }),
                                Celula::Texto(d.status.codigo().to_owned()),
                            ],
                        }
                    })
                    .collect(),
            },
            Area::Recorrentes => Tabela {
                colunas: colunas(&[
                    ("Recorrente", false),
                    ("Valor", true),
                    ("Frequência", false),
                    ("Ativo", false),
                ]),
                linhas: l
                    .recorrentes
                    .iter()
                    .map(|r| Linha {
                        alvo: Some(Ref::nova(Tipo::Recorrente, r.id.clone())),
                        celulas: vec![
                            self.link(Tipo::Recorrente, Some(&r.id)),
                            r.valor.map_or(Celula::Vazia, |v| self.valor(v.0, None)),
                            Celula::Texto(r.frequencia.codigo().to_owned()),
                            Celula::Texto(if r.ativo { "Sim" } else { "Não" }.to_owned()),
                        ],
                    })
                    .collect(),
            },
            Area::Centros => {
                let painel = painel_centros(l, &ciclo);
                Tabela {
                    colunas: colunas(&[("Centro", false), ("Saldo", true), ("No ciclo", true)]),
                    linhas: painel
                        .ativos
                        .iter()
                        .chain(&painel.arquivados)
                        .map(|c| {
                            let alvo = Ref::nova(Tipo::Centro, c.centro.id.clone());
                            let ligados = PedidoDeTabela::Ligados {
                                de: alvo.clone(),
                                tipo: Tipo::Movimentacao,
                            };
                            Linha {
                                alvo: Some(alvo),
                                celulas: vec![
                                    self.link(Tipo::Centro, Some(&c.centro.id)),
                                    self.valor(c.saldo.saldo.0, Some(ligados.clone())),
                                    self.valor(c.saldo.resultado_no_ciclo.0, Some(ligados)),
                                ],
                            }
                        })
                        .collect(),
                }
            }
            Area::FornecedoresDoLivro => Tabela {
                colunas: colunas(&[
                    ("Fornecedor", false),
                    ("Movimentações", true),
                    ("Saiu", true),
                ]),
                linhas: l
                    .fornecedores
                    .iter()
                    .map(|f| {
                        let alvo = Ref::nova(Tipo::Fornecedor, f.id.clone());
                        let (quantas, saiu) = {
                            let movs: Vec<&Movimentacao> = self.movimentacoes_de(&alvo).collect();
                            let saiu: i64 = movs
                                .iter()
                                .filter(|m| e_saida(m.tipo))
                                .map(|m| m.valor.0.abs())
                                .sum();
                            (movs.len(), saiu)
                        };
                        let ligados = PedidoDeTabela::Ligados {
                            de: alvo.clone(),
                            tipo: Tipo::Movimentacao,
                        };
                        Linha {
                            alvo: Some(alvo),
                            celulas: vec![
                                self.link(Tipo::Fornecedor, Some(&f.id)),
                                Celula::Contagem {
                                    n: i64::try_from(quantas).unwrap_or(i64::MAX),
                                    itens: Some(Box::new(ligados.clone())),
                                },
                                self.valor(-saiu, Some(ligados)),
                            ],
                        }
                    })
                    .collect(),
            },
            Area::Hoje | Area::Sincronizacao => Tabela::default(),
            outra => {
                return Err(ErroDeConsulta::NaoSuportado(format!(
                    "a área {outra:?} é do inventário"
                )));
            }
        })
    }

    fn itens_do_total(
        &self,
        de: &Ref,
        ciclo: Option<&str>,
        recorte: Option<&str>,
    ) -> ResultadoDeConsulta<Tabela> {
        let ciclo = ciclo.and_then(|c| Ciclo::do_nome(c, self.livro.corte));
        let no_ciclo = |m: &&Movimentacao| ciclo.as_ref().is_none_or(|c| c.contem(m.data));
        let l = &self.livro;
        let cartoes: Vec<&str> = l
            .contas
            .iter()
            .filter(|c| c.tipo == sbf_livro::TipoDeConta::CartaoDeCredito)
            .map(|c| c.id.as_str())
            .collect();
        let no_cartao =
            |m: &Movimentacao| m.fatura_id.is_some() || cartoes.contains(&m.conta_id.as_str());
        let movs = |f: &dyn Fn(&Movimentacao) -> bool| {
            self.tabela_de_movimentacoes(l.movimentacoes.iter().filter(no_ciclo).filter(|m| f(m)))
        };
        let entrou = |m: &Movimentacao| m.confirmada && e_entrada(m.tipo) && !no_cartao(m);
        let saiu = |m: &Movimentacao| m.confirmada && e_saida(m.tipo) && !no_cartao(m);
        let para_resolver = |motivo: MotivoParaResolver| {
            let painel = painel_hoje(l, self.hoje);
            let ids: Vec<&str> = painel
                .para_resolver
                .iter()
                .find(|i| i.motivo == motivo)
                .map(|i| i.movimentacoes.iter().map(String::as_str).collect())
                .unwrap_or_default();
            self.tabela_de_movimentacoes(
                l.movimentacoes
                    .iter()
                    .filter(|m| ids.contains(&m.id.as_str())),
            )
        };
        Ok(match (de.tipo, recorte) {
            (Tipo::Ciclo, Some("entrou")) => movs(&entrou),
            (Tipo::Ciclo, Some("saiu")) => movs(&saiu),
            (Tipo::Ciclo, Some("no-cartao")) => {
                movs(&|m| m.confirmada && e_saida(m.tipo) && no_cartao(m))
            }
            (Tipo::Ciclo, Some("resultado")) => movs(&|m| entrou(m) || saiu(m)),
            (Tipo::Ciclo, Some("sem-categoria")) => para_resolver(MotivoParaResolver::SemCategoria),
            (Tipo::Ciclo, Some("nao-alocadas")) => para_resolver(MotivoParaResolver::NaoAlocadas),
            (Tipo::Ciclo, Some("nao-confirmadas")) => {
                para_resolver(MotivoParaResolver::NaoConfirmadas)
            }
            (Tipo::Ciclo, Some("vencidos")) => para_resolver(MotivoParaResolver::Vencidas),
            (Tipo::Ciclo, Some("disponivel")) => {
                self.tabela_da_area(Area::ContasECartoes, &Filtro::default())?
            }
            (Tipo::Ciclo, Some("dividas")) => {
                self.tabela_da_area(Area::Dividas, &Filtro::default())?
            }
            (Tipo::Ciclo, Some("recorrentes")) => {
                self.tabela_da_area(Area::Recorrentes, &Filtro::default())?
            }
            (Tipo::Ciclo, Some("compromissos" | "ainda-vai-sair")) => {
                self.tabela_de_compromissos(recorte == Some("ainda-vai-sair"))
            }
            (Tipo::Fatura, Some("pagamento")) => {
                self.tabela_de_movimentacoes(l.movimentacoes.iter().filter(|m| {
                    m.tipo == TipoDeMovimentacao::PagamentoDeFatura
                        && m.fatura_id.as_deref() == Some(de.chave.as_str())
                }))
            }
            (Tipo::Fatura, _) => self.tabela_de_movimentacoes(l.movimentacoes.iter().filter(|m| {
                m.fatura_id.as_deref() == Some(de.chave.as_str())
                    && m.tipo != TipoDeMovimentacao::PagamentoDeFatura
            })),
            (Tipo::Centro, Some(r)) => {
                let quer = |m: &Movimentacao| match r {
                    "entradas" => e_entrada(m.tipo),
                    "saidas" => e_saida(m.tipo),
                    _ => m.tipo != TipoDeMovimentacao::Ajuste,
                };
                self.tabela_de_movimentacoes(
                    self.movimentacoes_de(de)
                        .filter(no_ciclo)
                        .filter(|m| quer(m)),
                )
            }
            (_, Some(outro))
                if matches!(
                    outro,
                    "origem"
                        | "proximos"
                        | "fornecedor-e-contrato"
                        | "simulacao"
                        | "compromissos-e-dividas"
                        | "parcelas-e-dividas"
                        | "documentos-a-revisar"
                        | "faturas-abertas"
                ) =>
            {
                return Err(ErroDeConsulta::NaoSuportado(format!(
                    "o recorte {outro} ainda não é lido do cache da estação"
                )));
            }
            // Conta, cartão, fornecedor, categoria…: as movimentações ligadas (sem ajuste).
            _ => self.tabela_de_movimentacoes(
                self.movimentacoes_de(de)
                    .filter(no_ciclo)
                    .filter(|m| m.tipo != TipoDeMovimentacao::Ajuste),
            ),
        })
    }

    fn tabela_de_compromissos(&self, so_os_que_faltam: bool) -> Tabela {
        let hoje = painel_hoje(&self.livro, self.hoje);
        Tabela {
            colunas: colunas(&[("Data", false), ("Compromisso", false), ("Valor", true)]),
            linhas: hoje
                .compromissos
                .iter()
                .filter(|c| !so_os_que_faltam || !c.vencido)
                .map(|c| Linha {
                    alvo: None,
                    celulas: vec![
                        Celula::Data(c.data.to_string()),
                        Celula::Texto(c.descricao.clone()),
                        self.valor(-c.valor.0, None),
                    ],
                })
                .collect(),
        }
    }

    /// A aba "Ligações": tudo que se liga a `de`, numa tabela mista.
    fn ligacoes(&self, de: &Ref) -> Tabela {
        let l = &self.livro;
        let mut refs: Vec<(Tipo, String)> = Vec::new();
        match de.tipo {
            Tipo::Movimentacao => {
                if let Some(m) = l.movimentacoes.iter().find(|m| m.id == de.chave) {
                    refs.push((Tipo::Conta, m.conta_id.clone()));
                    for (tipo, id) in [
                        (Tipo::Categoria, &m.categoria_id),
                        (Tipo::Fornecedor, &m.fornecedor_id),
                        (Tipo::Fatura, &m.fatura_id),
                        (Tipo::Parcela, &m.parcela_id),
                        (Tipo::Documento, &m.documento_id),
                    ] {
                        if let Some(id) = id {
                            refs.push((tipo, id.clone()));
                        }
                    }
                    refs.extend(
                        l.rateios
                            .iter()
                            .filter(|r| r.movimentacao_id == m.id)
                            .map(|r| (Tipo::Centro, r.centro_id.clone())),
                    );
                }
            }
            _ => refs.extend(
                self.movimentacoes_de(de)
                    .map(|m| (Tipo::Movimentacao, m.id.clone())),
            ),
        }
        Tabela {
            colunas: colunas(&[("Tipo", false), ("Item", false)]),
            linhas: refs
                .into_iter()
                .map(|(tipo, id)| {
                    let texto = match tipo {
                        Tipo::Movimentacao => l
                            .movimentacoes
                            .iter()
                            .find(|m| m.id == id)
                            .map_or_else(|| id.clone(), descricao),
                        _ => self
                            .nome_de(tipo, &id)
                            .map_or_else(|| id.clone(), str::to_owned),
                    };
                    let alvo = Ref::nova(tipo, id);
                    Linha {
                        alvo: Some(alvo.clone()),
                        celulas: vec![
                            Celula::Texto(format!("{tipo:?}")),
                            Celula::Link { alvo, texto },
                        ],
                    }
                })
                .collect(),
        }
    }

    // ---------------- painéis ----------------

    fn indicador(&self, rotulo: &str, valor: Celula, nota: Option<String>) -> Indicador {
        Indicador {
            rotulo: rotulo.to_owned(),
            valor,
            nota,
            estimativa: None,
            instantaneo: false,
        }
    }

    fn disponivel(&self, hoje: &PainelHoje) -> Indicador {
        Indicador {
            instantaneo: true,
            nota: Some(format!("{} contas", hoje.contas.len())),
            ..self.indicador(
                "Disponível agora",
                self.valor(
                    hoje.disponivel_agora.0,
                    Some(self.itens(
                        Ref::nova(Tipo::Ciclo, hoje.ciclo.nome()),
                        None,
                        RECORTE_DISPONIVEL,
                    )),
                ),
                None,
            )
        }
    }

    fn resultado(&self, hoje: &PainelHoje) -> Vec<Indicador> {
        let ciclo = &hoje.ciclo;
        let de = Ref::nova(Tipo::Ciclo, ciclo.nome());
        let r = &hoje.resultado;
        vec![
            self.indicador(
                "Resultado do mês",
                self.valor(r.entrou.0 - r.saiu.0, None),
                Some(ciclo.rotulo()),
            ),
            self.indicador(
                "Entrou",
                self.valor(
                    r.entrou.0,
                    Some(self.itens(de.clone(), Some(ciclo), RECORTE_ENTROU)),
                ),
                None,
            ),
            self.indicador(
                "Saiu",
                self.valor(
                    -r.saiu.0,
                    Some(self.itens(de.clone(), Some(ciclo), RECORTE_SAIU)),
                ),
                None,
            ),
            self.indicador(
                "No cartão",
                self.valor(
                    -r.no_cartao.0,
                    Some(self.itens(de, Some(ciclo), RECORTE_NO_CARTAO)),
                ),
                None,
            ),
        ]
    }

    fn secao_cartoes(&self, hoje: &PainelHoje) -> Secao {
        Secao {
            titulo: "Cartões".to_owned(),
            nota: Some("Fatura atual: total menos o pago da fatura aberta".to_owned()),
            tabela: Tabela {
                colunas: colunas(&[
                    ("Cartão", false),
                    ("Fatura atual", true),
                    ("Limite livre", true),
                    ("Vencimento", false),
                ]),
                linhas: hoje
                    .cartoes
                    .iter()
                    .map(|c| Linha {
                        alvo: Some(Ref::nova(Tipo::Conta, c.conta_id.clone())),
                        celulas: vec![
                            Celula::Link {
                                alvo: Ref::nova(Tipo::Conta, c.conta_id.clone()),
                                texto: c.nome.clone(),
                            },
                            self.valor(
                                -c.fatura_atual.0,
                                c.fatura_id.as_ref().map(|f| PedidoDeTabela::Ligados {
                                    de: Ref::nova(Tipo::Fatura, f.clone()),
                                    tipo: Tipo::Movimentacao,
                                }),
                            ),
                            c.limite_livre
                                .map_or(Celula::Vazia, |v| self.valor(v.0, None)),
                            c.vencimento
                                .map_or(Celula::Vazia, |d| Celula::Data(d.to_string())),
                        ],
                    })
                    .collect(),
            },
        }
    }

    fn painel_hoje_da_area(&self) -> PainelDeArea {
        let hoje = painel_hoje(&self.livro, self.hoje);
        let mut indicadores = vec![self.disponivel(&hoje)];
        indicadores.push(Indicador {
            estimativa: hoje
                .estimativa
                .then(|| "há compromisso sem valor esperado".to_owned()),
            ..self.indicador(
                "Livre após compromissos",
                self.valor(hoje.livre_apos_compromissos.0, None),
                None,
            )
        });
        indicadores.push(self.indicador(
            "Ainda vai sair",
            self.valor(-hoje.resultado.ainda_vai_sair.0, None),
            Some(format!(
                "falta até o fim: {}",
                crate::texto::dinheiro(hoje.falta_ate_o_fim_do_ciclo.0)
            )),
        ));
        indicadores.extend(self.resultado(&hoje));
        indicadores.push(self.indicador(
            "Dívida total",
            self.valor(hoje.divida_total.0, None),
            Some(format!(
                "{} por mês",
                crate::texto::dinheiro(hoje.compromisso_mensal_de_dividas.0)
            )),
        ));
        let de = Ref::nova(Tipo::Ciclo, hoje.ciclo.nome());
        let para_resolver = Secao {
            titulo: "Para resolver".to_owned(),
            nota: None,
            tabela: Tabela {
                colunas: colunas(&[("O quê", false), ("Quantas", true)]),
                linhas: hoje
                    .para_resolver
                    .iter()
                    .map(|i| Linha {
                        alvo: None,
                        celulas: vec![
                            Celula::Texto(i.motivo.rotulo().to_owned()),
                            Celula::Contagem {
                                n: i64::from(i.quantidade),
                                itens: (!i.movimentacoes.is_empty()).then(|| {
                                    Box::new(self.itens(de.clone(), None, recorte_de(i.motivo)))
                                }),
                            },
                        ],
                    })
                    .collect(),
            },
        };
        let compromissos = Secao {
            titulo: "Compromissos até o fim do ciclo".to_owned(),
            nota: None,
            tabela: Tabela {
                colunas: colunas(&[("Data", false), ("Compromisso", false), ("Valor", true)]),
                linhas: hoje
                    .compromissos
                    .iter()
                    .map(|c| Linha {
                        alvo: None,
                        celulas: vec![
                            Celula::Data(c.data.to_string()),
                            Celula::Texto(c.descricao.clone()),
                            self.valor(-c.valor.0, None),
                        ],
                    })
                    .collect(),
            },
        };
        PainelDeArea {
            ciclo: Some(hoje.ciclo.nome()),
            indicadores,
            secoes: vec![para_resolver, self.secao_cartoes(&hoje), compromissos],
            avisos: Vec::new(),
        }
    }

    fn painel_centros_da_area(&self) -> PainelDeArea {
        let ciclo = self.ciclo();
        let painel = painel_centros(&self.livro, &ciclo);
        let n = &painel.nao_alocado;
        PainelDeArea {
            ciclo: Some(ciclo.nome()),
            indicadores: vec![self.indicador(
                "Não alocado",
                self.valor(-n.total.0, None),
                Some(format!("{} movimentações", n.movimentacoes)),
            )],
            secoes: vec![Secao {
                titulo: "Não alocado".to_owned(),
                nota: Some("Saídas do ciclo sem rateio completo".to_owned()),
                tabela: self.tabela_de_movimentacoes(
                    self.livro
                        .movimentacoes
                        .iter()
                        .filter(|m| n.pendentes.iter().any(|p| p.id == m.id)),
                ),
            }],
            avisos: Vec::new(),
        }
    }
}

fn colunas(c: &[(&str, bool)]) -> Vec<Coluna> {
    c.iter()
        .map(|(rotulo, numerica)| Coluna {
            rotulo: (*rotulo).to_owned(),
            numerica: *numerica,
        })
        .collect()
}

fn descricao(m: &Movimentacao) -> String {
    m.descricao
        .clone()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| "Movimentação".to_owned())
}

/// Entrada positiva, saída negativa; o ajuste fica como está (Q15).
fn com_sinal(m: &Movimentacao) -> i64 {
    let v = m.valor.0.abs();
    if e_entrada(m.tipo) {
        v
    } else if e_saida(m.tipo) {
        -v
    } else {
        m.valor.0
    }
}

impl Consulta for ConsultaDoLivro {
    fn tabela(&self, pedido: &PedidoDeTabela) -> ResultadoDeConsulta<Tabela> {
        match pedido {
            PedidoDeTabela::Area { area, filtro } => self.tabela_da_area(*area, filtro),
            PedidoDeTabela::Ligados { de, tipo } if *tipo == de.tipo => Ok(self.ligacoes(de)),
            PedidoDeTabela::Ligados { de, tipo } => match tipo {
                Tipo::Movimentacao => Ok(self.tabela_de_movimentacoes(self.movimentacoes_de(de))),
                Tipo::Centro if de.tipo == Tipo::Movimentacao => Ok(Tabela {
                    colunas: colunas(&[("Centro", false), ("Valor", true)]),
                    linhas: self
                        .livro
                        .rateios
                        .iter()
                        .filter(|r| r.movimentacao_id == de.chave)
                        .map(|r| Linha {
                            alvo: Some(Ref::nova(Tipo::Centro, r.centro_id.clone())),
                            celulas: vec![
                                self.link(Tipo::Centro, Some(&r.centro_id)),
                                self.valor(r.valor.0, None),
                            ],
                        })
                        .collect(),
                }),
                outro => Err(ErroDeConsulta::NaoSuportado(format!(
                    "itens de {outro:?} ligados a {:?}",
                    de.tipo
                ))),
            },
            PedidoDeTabela::ItensDoTotal {
                de, ciclo, recorte, ..
            } => self.itens_do_total(de, ciclo.as_deref(), recorte.as_deref()),
        }
    }

    fn item(&self, alvo: &Ref) -> ResultadoDeConsulta<Option<Item>> {
        let l = &self.livro;
        let campo = |rotulo: &str, valor: Celula| Campo {
            rotulo: rotulo.to_owned(),
            valor,
        };
        let ligados = PedidoDeTabela::Ligados {
            de: alvo.clone(),
            tipo: Tipo::Movimentacao,
        };
        Ok(match alvo.tipo {
            Tipo::Movimentacao => l
                .movimentacoes
                .iter()
                .find(|m| m.id == alvo.chave)
                .map(|m| Item {
                    alvo: alvo.clone(),
                    rotulo: format!("{} · {}", descricao(m), dia_e_mes(&m.data.to_string())),
                    campos: vec![
                        campo("Valor", self.valor(com_sinal(m), None)),
                        campo("Data", Celula::Data(m.data.to_string())),
                        campo("Tipo", Celula::Texto(m.tipo.codigo().to_owned())),
                        campo("Conta", self.link(Tipo::Conta, Some(&m.conta_id))),
                        campo(
                            "Categoria",
                            self.link(Tipo::Categoria, m.categoria_id.as_deref()),
                        ),
                        campo(
                            "Fornecedor",
                            self.link(Tipo::Fornecedor, m.fornecedor_id.as_deref()),
                        ),
                        campo(
                            "Confirmada",
                            Celula::Texto(if m.confirmada { "Sim" } else { "Não" }.to_owned()),
                        ),
                    ],
                }),
            Tipo::Conta => l.contas.iter().find(|c| c.id == alvo.chave).map(|c| Item {
                alvo: alvo.clone(),
                rotulo: c.nome.clone(),
                campos: vec![
                    campo("Saldo", self.valor(c.saldo.0, Some(ligados.clone()))),
                    campo(
                        "Instituição",
                        self.link(Tipo::Instituicao, Some(&c.instituicao_id)),
                    ),
                    campo("Tipo", Celula::Texto(c.tipo.codigo().to_owned())),
                ],
            }),
            Tipo::Divida => l.dividas.iter().find(|d| d.id == alvo.chave).map(|d| Item {
                alvo: alvo.clone(),
                rotulo: d.nome.clone(),
                campos: vec![
                    campo(
                        "Saldo devedor",
                        self.valor(d.saldo_devedor.0, Some(ligados.clone())),
                    ),
                    campo("Valor original", self.valor(d.valor_original.0, None)),
                    campo("Conta", self.link(Tipo::Conta, Some(&d.conta_id))),
                ],
            }),
            Tipo::Categoria | Tipo::Centro | Tipo::Fornecedor | Tipo::Recorrente => {
                self.nome_de(alvo.tipo, &alvo.chave).map(|nome| {
                    let total: i64 = self.movimentacoes_de(alvo).map(com_sinal).sum();
                    Item {
                        alvo: alvo.clone(),
                        rotulo: nome.to_owned(),
                        campos: vec![campo("Total", self.valor(total, Some(ligados.clone())))],
                    }
                })
            }
            _ => None,
        })
    }

    fn rastro(&self, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
        if alvo.tipo != Tipo::Movimentacao {
            return Ok(Rastro::default());
        }
        let l = &self.livro;
        let Some(m) = l.movimentacoes.iter().find(|m| m.id == alvo.chave) else {
            return Err(ErroDeConsulta::NaoEncontrado(alvo.clone()));
        };
        let elo = |tipo: Tipo, id: &str, rotulo: String, papel: &str| Elo {
            alvo: Ref::nova(tipo, id),
            rotulo,
            papel: papel.to_owned(),
            quem: None,
            quando: None,
        };
        let mut acima = vec![elo(
            Tipo::Conta,
            &m.conta_id,
            self.nome_de(Tipo::Conta, &m.conta_id)
                .unwrap_or("Conta")
                .to_owned(),
            "na conta",
        )];
        if let Some(doc) = &m.documento_id {
            acima.push(elo(
                Tipo::Documento,
                doc,
                "Documento".to_owned(),
                "do documento",
            ));
        }
        let abaixo = l
            .rateios
            .iter()
            .filter(|r| r.movimentacao_id == m.id)
            .map(|r| {
                elo(
                    Tipo::Centro,
                    &r.centro_id,
                    format!(
                        "{} · {}",
                        self.nome_de(Tipo::Centro, &r.centro_id).unwrap_or("Centro"),
                        crate::texto::dinheiro(r.valor.0)
                    ),
                    "rateada para",
                )
            })
            .collect();
        Ok(Rastro { acima, abaixo })
    }

    fn por_ciclo(
        &self,
        alvo: &Ref,
        _fonte: sbf_inventario::consulta::Fonte,
        ciclos: usize,
    ) -> ResultadoDeConsulta<Vec<TotalDoCiclo>> {
        let mut ciclo = self.ciclo();
        let mut lista = Vec::with_capacity(ciclos);
        for _ in 0..ciclos {
            lista.push(ciclo);
            ciclo = ciclo.anterior();
        }
        lista.reverse();
        Ok(lista
            .into_iter()
            .map(|c| TotalDoCiclo {
                ciclo: c.nome(),
                centavos: self
                    .movimentacoes_de(alvo)
                    .filter(|m| c.contem(m.data))
                    .map(com_sinal)
                    .sum(),
                itens: Some(Box::new(PedidoDeTabela::ItensDoTotal {
                    de: alvo.clone(),
                    fonte: sbf_inventario::consulta::Fonte::LivroCaixa,
                    ciclo: Some(c.nome()),
                    recorte: None,
                })),
            })
            .collect())
    }

    fn visao_geral(&self) -> ResultadoDeConsulta<Painel> {
        Ok(Painel::default())
    }

    fn relatorio(
        &self,
        _pergunta: Pergunta,
        _fonte: sbf_inventario::consulta::Fonte,
        _filtro: &Filtro,
    ) -> ResultadoDeConsulta<Painel> {
        Err(ErroDeConsulta::NaoSuportado(
            "os Relatórios do livro-caixa usam `responder_com` com `ConsultaDoLivro::gastos`"
                .to_owned(),
        ))
    }

    fn contagens(&self) -> ResultadoDeConsulta<Contagens> {
        Ok(Contagens::default())
    }

    fn texto(&self, _achado: &Ref) -> ResultadoDeConsulta<Option<String>> {
        Ok(None)
    }

    fn painel_da_area(&self, area: Area, filtro: &Filtro) -> ResultadoDeConsulta<PainelDeArea> {
        let _ = filtro;
        Ok(match area {
            Area::Hoje => self.painel_hoje_da_area(),
            Area::Centros => self.painel_centros_da_area(),
            Area::Movimentacoes => {
                let hoje = painel_hoje(&self.livro, self.hoje);
                PainelDeArea {
                    ciclo: Some(hoje.ciclo.nome()),
                    indicadores: self.resultado(&hoje),
                    ..PainelDeArea::default()
                }
            }
            Area::ContasECartoes => {
                let hoje = painel_hoje(&self.livro, self.hoje);
                PainelDeArea {
                    ciclo: Some(hoje.ciclo.nome()),
                    indicadores: vec![self.disponivel(&hoje)],
                    secoes: vec![self.secao_cartoes(&hoje)],
                    avisos: Vec::new(),
                }
            }
            Area::Dividas => {
                let hoje = painel_hoje(&self.livro, self.hoje);
                PainelDeArea {
                    ciclo: None,
                    indicadores: vec![self.indicador(
                        "Dívida total",
                        self.valor(hoje.divida_total.0, None),
                        Some(format!(
                            "{} por mês",
                            crate::texto::dinheiro(hoje.compromisso_mensal_de_dividas.0)
                        )),
                    )],
                    ..PainelDeArea::default()
                }
            }
            Area::Recorrentes | Area::FornecedoresDoLivro => PainelDeArea::default(),
            outra => {
                return Err(ErroDeConsulta::NaoSuportado(format!(
                    "a área {outra:?} não é do livro-caixa"
                )));
            }
        })
    }
}
