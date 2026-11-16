//! Cache das leituras de um quadro para o outro. A janela redesenha a 60 quadros por
//! segundo; a `Consulta` só é chamada quando o pedido muda ou depois de uma decisão
//! (`limpar`). Erros também ficam em cache, para não martelar o banco.

use std::collections::HashMap;
use std::sync::Arc;

use sbf_inventario::Consulta;
use sbf_inventario::Ref;
use sbf_inventario::consulta::{
    Area, Contagens, Filtro, Fonte, Item, Painel, PainelDeArea, PedidoDeTabela, Pergunta, Rastro,
    Tabela, TotalDoCiclo,
};

/// Resultado de leitura já pronto para desenhar: o erro vira texto.
pub type Lido<T> = Result<Arc<T>, String>;

/// Quantas linhas um pedaço traz.
pub const PEDACO: usize = 1000;

/// Uma tabela lida em pedaços: as primeiras linhas e o total.
#[derive(Debug, Clone)]
pub struct Carregada {
    pub tabela: Arc<Tabela>,
    pub total: usize,
}

#[derive(Debug, Default)]
pub struct Cache {
    tabelas: HashMap<String, Lido<Tabela>>,
    paginadas: HashMap<String, Result<Carregada, String>>,
    itens: HashMap<Ref, Lido<Option<Item>>>,
    rastros: HashMap<Ref, Lido<Rastro>>,
    por_ciclo: HashMap<(Ref, bool, usize), Lido<Vec<TotalDoCiclo>>>,
    textos: HashMap<Ref, Lido<Option<String>>>,
    paineis: HashMap<String, Lido<Painel>>,
    paineis_de_area: HashMap<String, Lido<PainelDeArea>>,
    contagens: Option<Lido<Contagens>>,
}

pub fn chave<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

impl Cache {
    /// Esquece tudo: a próxima leitura vai à `Consulta`.
    pub fn limpar(&mut self) {
        *self = Cache::default();
    }
}

/// A `Consulta` com cache, para o tempo de um quadro.
pub struct Leitor<'a> {
    pub consulta: &'a dyn Consulta,
    pub cache: &'a mut Cache,
}

impl std::fmt::Debug for Leitor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Leitor").finish_non_exhaustive()
    }
}

impl<'a> Leitor<'a> {
    pub fn novo(consulta: &'a dyn Consulta, cache: &'a mut Cache) -> Self {
        Self { consulta, cache }
    }

    pub fn tabela(&mut self, pedido: &PedidoDeTabela) -> Lido<Tabela> {
        let k = chave(pedido);
        let consulta = self.consulta;
        self.cache
            .tabelas
            .entry(k)
            .or_insert_with(|| {
                consulta
                    .tabela(pedido)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .clone()
    }

    /// A tabela em pedaços (`Consulta::tabela_paginada`), com pelo menos `minimo` linhas
    /// carregadas (ou todas, se a tabela tem menos). O pedaço novo entra no fim, sem copiar
    /// o que já estava carregado.
    pub fn carregada(
        &mut self,
        pedido: &PedidoDeTabela,
        minimo: usize,
    ) -> Result<Carregada, String> {
        let k = chave(pedido);
        let consulta = self.consulta;
        let entrada = self.cache.paginadas.entry(k).or_insert_with(|| {
            consulta
                .tabela_paginada(pedido, 0, minimo.max(PEDACO))
                .map(|p| Carregada {
                    tabela: Arc::new(p.tabela),
                    total: p.total,
                })
                .map_err(|e| e.to_string())
        });
        if let Ok(c) = entrada {
            let carregadas = c.tabela.linhas.len();
            if carregadas < minimo.min(c.total) {
                let falta = (minimo.min(c.total) - carregadas).div_ceil(PEDACO) * PEDACO;
                match consulta.tabela_paginada(pedido, carregadas, falta) {
                    Ok(p) => {
                        c.total = p.total;
                        Arc::make_mut(&mut c.tabela).linhas.extend(p.tabela.linhas);
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        entrada.clone()
    }

    pub fn item(&mut self, alvo: &Ref) -> Lido<Option<Item>> {
        let consulta = self.consulta;
        self.cache
            .itens
            .entry(alvo.clone())
            .or_insert_with(|| consulta.item(alvo).map(Arc::new).map_err(|e| e.to_string()))
            .clone()
    }

    pub fn rastro(&mut self, alvo: &Ref) -> Lido<Rastro> {
        let consulta = self.consulta;
        self.cache
            .rastros
            .entry(alvo.clone())
            .or_insert_with(|| {
                consulta
                    .rastro(alvo)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .clone()
    }

    pub fn por_ciclo(
        &mut self,
        alvo: &Ref,
        fonte: Fonte,
        ciclos: usize,
    ) -> Lido<Vec<TotalDoCiclo>> {
        let consulta = self.consulta;
        self.cache
            .por_ciclo
            .entry((alvo.clone(), fonte == Fonte::Inventario, ciclos))
            .or_insert_with(|| {
                consulta
                    .por_ciclo(alvo, fonte, ciclos)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .clone()
    }

    pub fn texto(&mut self, achado: &Ref) -> Lido<Option<String>> {
        let consulta = self.consulta;
        self.cache
            .textos
            .entry(achado.clone())
            .or_insert_with(|| {
                consulta
                    .texto(achado)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .clone()
    }

    pub fn visao_geral(&mut self) -> Lido<Painel> {
        let consulta = self.consulta;
        self.cache
            .paineis
            .entry("visao-geral".to_owned())
            .or_insert_with(|| {
                consulta
                    .visao_geral()
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .clone()
    }

    /// Já há um pedaço desta tabela em cache?
    pub fn tem_pedaco(&self, pedido: &PedidoDeTabela) -> bool {
        self.cache.paginadas.contains_key(&chave(pedido))
    }

    /// Guarda o primeiro pedaço de uma tabela lido em segundo plano.
    pub fn guardar_pedaco(
        &mut self,
        pedido: &PedidoDeTabela,
        r: Result<sbf_inventario::consulta::PaginaDeTabela, String>,
    ) {
        let c = r.map(|p| Carregada {
            tabela: Arc::new(p.tabela),
            total: p.total,
        });
        self.cache.paginadas.insert(chave(pedido), c);
    }

    /// Guarda uma Visão geral lida em segundo plano.
    pub fn guardar_visao_geral(&mut self, r: Result<Painel, String>) {
        self.cache
            .paineis
            .insert("visao-geral".to_owned(), r.map(Arc::new));
    }

    pub fn relatorio(&mut self, pergunta: Pergunta, fonte: Fonte, filtro: &Filtro) -> Lido<Painel> {
        let k = chave(&(pergunta, fonte, filtro));
        let consulta = self.consulta;
        self.cache
            .paineis
            .entry(k)
            .or_insert_with(|| {
                consulta
                    .relatorio(pergunta, fonte, filtro)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .clone()
    }

    /// Os números e as seções de uma área de Dinheiro.
    pub fn painel_da_area(&mut self, area: Area, filtro: &Filtro) -> Lido<PainelDeArea> {
        let k = chave(&(area, filtro));
        let consulta = self.consulta;
        self.cache
            .paineis_de_area
            .entry(k)
            .or_insert_with(|| {
                consulta
                    .painel_da_area(area, filtro)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .clone()
    }

    pub fn contagens(&mut self) -> Lido<Contagens> {
        let consulta = self.consulta;
        self.cache
            .contagens
            .get_or_insert_with(|| {
                consulta
                    .contagens()
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .clone()
    }

    /// O nome de um item para a trilha e os títulos; cai na chave se não houver.
    pub fn rotulo(&mut self, alvo: &Ref) -> String {
        match self.item(alvo) {
            Ok(i) => match i.as_ref() {
                Some(item) => item.rotulo.clone(),
                None => alvo.chave.clone(),
            },
            Err(_) => alvo.chave.clone(),
        }
    }
}
