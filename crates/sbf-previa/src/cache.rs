//! O cache rotativo da prévia, **só em memória** (spec 06: as últimas 30 prévias ou até
//! 256 MB, o que vier primeiro). Nada vai para disco.
//!
//! É um LRU por documento (chave `content_hash`): cada documento conta os bytes do seu
//! conteúdo ([`Conteudo::bytes`]) mais os das páginas já desenhadas dele. Ao passar de um
//! limite, descarta o documento usado há mais tempo, inteiro (conteúdo e páginas). O
//! documento que acabou de ser usado nunca é descartado, mesmo sozinho acima de `max_bytes`:
//! a janela está mostrando ele.
//!
//! Não é thread-safe por si: `obter` e `guardar` pedem `&mut self`. A janela guarda o cache
//! num `Mutex` e não segura a trava enquanto desenha (desenhar é lento). O que sai é `Arc`:
//! o conteúdo continua vivo enquanto alguém o usa, mesmo depois de descartado do cache.

use crate::{Conteudo, Imagem};
use std::collections::HashMap;
use std::sync::Arc;

struct Entrada {
    conteudo: Arc<Conteudo>,
    bytes_do_conteudo: usize,
    /// (página, largura) → imagem. A imagem de um [`Conteudo::Imagem`] usa a página 1.
    imagens: HashMap<(usize, u32), Arc<Imagem>>,
    bytes_das_imagens: usize,
    /// O "relógio" do último uso.
    uso: u64,
}

impl Entrada {
    fn bytes(&self) -> usize {
        self.bytes_do_conteudo + self.bytes_das_imagens
    }
}

/// Cache rotativo **só em memória**: as últimas `max_documentos` prévias ou até
/// `max_bytes`, o que vier primeiro (padrão 30 e 256 MB). A chave é o `content_hash`.
pub struct Cache {
    max_documentos: usize,
    max_bytes: usize,
    entradas: HashMap<String, Entrada>,
    relogio: u64,
    bytes: usize,
}

impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cache")
            .field("max_documentos", &self.max_documentos)
            .field("max_bytes", &self.max_bytes)
            .field("documentos", &self.entradas.len())
            .field("bytes", &self.bytes)
            .finish()
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::novo(30, 256 * 1024 * 1024)
    }
}

impl Cache {
    pub fn novo(max_documentos: usize, max_bytes: usize) -> Self {
        Self {
            max_documentos,
            max_bytes,
            entradas: HashMap::new(),
            relogio: 0,
            bytes: 0,
        }
    }

    pub fn limites(&self) -> (usize, usize) {
        (self.max_documentos, self.max_bytes)
    }

    /// Troca os limites (Opções) e descarta o que passar deles.
    pub fn ajustar(&mut self, max_documentos: usize, max_bytes: usize) {
        self.max_documentos = max_documentos;
        self.max_bytes = max_bytes;
        self.descartar_excesso(None);
    }

    /// Quantos documentos estão guardados.
    pub fn documentos(&self) -> usize {
        self.entradas.len()
    }

    /// Esquece tudo (o chaveiro foi trancado: o PDF decifrado não fica em memória).
    pub fn limpar(&mut self) {
        self.entradas.clear();
        self.bytes = 0;
    }

    fn tique(&mut self) -> u64 {
        self.relogio += 1;
        self.relogio
    }

    /// O conteúdo aberto de um documento, marcando-o como o mais recente.
    pub fn obter(&mut self, chave: &str) -> Option<Arc<Conteudo>> {
        let agora = self.tique();
        let entrada = self.entradas.get_mut(chave)?;
        entrada.uso = agora;
        Some(entrada.conteudo.clone())
    }

    /// Guarda o conteúdo e descarta os mais antigos além dos limites. Guardar de novo uma
    /// chave troca o conteúdo e esquece as páginas desenhadas.
    pub fn guardar(&mut self, chave: String, conteudo: Conteudo) -> Arc<Conteudo> {
        let agora = self.tique();
        let bytes_do_conteudo = conteudo.bytes();
        let conteudo = Arc::new(conteudo);
        let antiga = self.entradas.insert(
            chave.clone(),
            Entrada {
                conteudo: conteudo.clone(),
                bytes_do_conteudo,
                imagens: HashMap::new(),
                bytes_das_imagens: 0,
                uso: agora,
            },
        );
        if let Some(antiga) = antiga {
            self.bytes -= antiga.bytes();
        }
        self.bytes += bytes_do_conteudo;
        self.descartar_excesso(Some(&chave));
        conteudo
    }

    /// Uma página (ou a imagem) já desenhada nessa largura.
    pub fn imagem(&mut self, chave: &str, pagina: usize, largura_px: u32) -> Option<Arc<Imagem>> {
        let agora = self.tique();
        let entrada = self.entradas.get_mut(chave)?;
        let imagem = entrada.imagens.get(&(pagina, largura_px))?.clone();
        entrada.uso = agora;
        Some(imagem)
    }

    /// Guarda uma página desenhada; conta no limite de bytes do documento. Se o documento
    /// não está no cache (já foi descartado), só devolve a imagem.
    pub fn guardar_imagem(
        &mut self,
        chave: &str,
        pagina: usize,
        largura_px: u32,
        imagem: Imagem,
    ) -> Arc<Imagem> {
        let agora = self.tique();
        let imagem = Arc::new(imagem);
        let Some(entrada) = self.entradas.get_mut(chave) else {
            return imagem;
        };
        entrada.uso = agora;
        let bytes = imagem.bytes();
        if let Some(antiga) = entrada.imagens.insert((pagina, largura_px), imagem.clone()) {
            entrada.bytes_das_imagens -= antiga.bytes();
            self.bytes -= antiga.bytes();
        }
        entrada.bytes_das_imagens += bytes;
        self.bytes += bytes;
        self.descartar_excesso(Some(chave));
        imagem
    }

    /// Bytes ocupados agora.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Descarta o usado há mais tempo até caber nos limites, poupando `poupar`.
    fn descartar_excesso(&mut self, poupar: Option<&str>) {
        while self.entradas.len() > self.max_documentos || self.bytes > self.max_bytes {
            let mais_antigo = self
                .entradas
                .iter()
                .filter(|(chave, _)| Some(chave.as_str()) != poupar)
                .min_by_key(|(_, e)| e.uso)
                .map(|(chave, _)| chave.clone());
            let Some(chave) = mais_antigo else {
                break;
            };
            if let Some(entrada) = self.entradas.remove(&chave) {
                self.bytes -= entrada.bytes();
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn texto(bytes: usize) -> Conteudo {
        Conteudo::Texto("x".repeat(bytes))
    }

    fn imagem(bytes: usize) -> Imagem {
        Imagem {
            largura: 1,
            altura: (bytes / 4) as u32,
            rgba: vec![0; bytes],
        }
    }

    #[test]
    fn descarta_o_mais_antigo_alem_do_limite_de_documentos() {
        let mut cache = Cache::novo(2, 1_000_000);
        cache.guardar("a".into(), texto(10));
        cache.guardar("b".into(), texto(10));
        // Usar "a" faz de "b" o mais antigo.
        assert!(cache.obter("a").is_some());
        cache.guardar("c".into(), texto(10));
        assert_eq!(cache.documentos(), 2);
        assert!(cache.obter("b").is_none());
        assert!(cache.obter("a").is_some());
        assert!(cache.obter("c").is_some());
        assert_eq!(cache.bytes(), 20);
    }

    #[test]
    fn descarta_pelo_limite_de_bytes_contando_as_paginas_desenhadas() {
        let mut cache = Cache::novo(30, 1000);
        cache.guardar("a".into(), texto(300));
        cache.guardar("b".into(), texto(300));
        assert_eq!(cache.bytes(), 600);
        // 404 bytes de página em "b": passa de 1000 e "a" (o mais antigo) sai.
        cache.guardar_imagem("b", 1, 100, imagem(404));
        assert_eq!(cache.bytes(), 704);
        assert!(cache.obter("a").is_none());
        assert!(cache.imagem("b", 1, 100).is_some());
        assert!(cache.imagem("b", 1, 200).is_none());
        // Trocar a mesma página não conta duas vezes.
        cache.guardar_imagem("b", 1, 100, imagem(200));
        assert_eq!(cache.bytes(), 500);
    }

    #[test]
    fn o_documento_em_uso_fica_mesmo_acima_do_limite() {
        let mut cache = Cache::novo(30, 100);
        cache.guardar("a".into(), texto(50));
        let grande = cache.guardar("b".into(), texto(500));
        assert_eq!(grande.bytes(), 500);
        assert!(cache.obter("a").is_none());
        assert!(cache.obter("b").is_some());
        assert_eq!(cache.bytes(), 500);
        // Guardar outro descarta "b".
        cache.guardar("c".into(), texto(10));
        assert_eq!(cache.documentos(), 1);
        assert_eq!(cache.bytes(), 10);
    }

    #[test]
    fn imagem_de_documento_fora_do_cache_nao_conta() {
        let mut cache = Cache::novo(2, 1000);
        let i = cache.guardar_imagem("nada", 1, 10, imagem(40));
        assert_eq!(i.bytes(), 40);
        assert_eq!(cache.bytes(), 0);
        assert!(cache.imagem("nada", 1, 10).is_none());
    }

    #[test]
    fn regravar_a_chave_troca_o_conteudo_e_esquece_as_paginas() {
        let mut cache = Cache::novo(2, 1000);
        cache.guardar("a".into(), texto(100));
        cache.guardar_imagem("a", 1, 10, imagem(40));
        cache.guardar("a".into(), texto(30));
        assert_eq!(cache.bytes(), 30);
        assert!(cache.imagem("a", 1, 10).is_none());
        cache.ajustar(0, 0);
        assert_eq!(cache.documentos(), 0);
        assert_eq!(cache.bytes(), 0);
        assert_eq!(cache.limites(), (0, 0));
    }
}
