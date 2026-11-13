//! Cache das leituras de um quadro para o outro. A janela redesenha a 60 quadros por
//! segundo; a `Consulta` só é chamada quando o pedido muda ou depois de uma decisão
//! (`limpar`). Erros também ficam em cache, para não martelar o banco.

use std::collections::HashMap;
use std::sync::Arc;

use sbf_inventario::Consulta;
use sbf_inventario::Ref;
use sbf_inventario::consulta::{
    Contagens, Filtro, Fonte, Item, Painel, PedidoDeTabela, Pergunta, Rastro, Tabela, TotalDoCiclo,
};

/// Resultado de leitura já pronto para desenhar: o erro vira texto.
pub type Lido<T> = Result<Arc<T>, String>;

#[derive(Debug, Default)]
pub struct Cache {
    tabelas: HashMap<String, Lido<Tabela>>,
    itens: HashMap<Ref, Lido<Option<Item>>>,
    rastros: HashMap<Ref, Lido<Rastro>>,
    por_ciclo: HashMap<(Ref, bool, usize), Lido<Vec<TotalDoCiclo>>>,
    textos: HashMap<Ref, Lido<Option<String>>>,
    paineis: HashMap<String, Lido<Painel>>,
    contagens: Option<Lido<Contagens>>,
}

fn chave<T: serde::Serialize>(v: &T) -> String {
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
