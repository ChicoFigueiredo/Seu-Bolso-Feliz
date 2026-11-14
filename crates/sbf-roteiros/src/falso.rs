//! O navegador falso: um site em memória, sem Chrome. Serve aos testes do executor e da
//! gravação e à demonstração da janela. Guarda o que foi digitado só para o teste conferir
//! que o segredo chegou ao campo (é código de teste, com dados fictícios).

use crate::bloqueio::{Bloqueio, Candidato};
use crate::erro::{ErroDeRoteiro, Resultado};
use crate::navegador::Navegador;
use crate::pagina::Pagina;
use sbf_chaveiro::Segredo;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// O que um clique ou uma navegação faz no site falso.
#[derive(Debug, Clone)]
pub struct Elemento {
    pub candidato: Candidato,
    /// A página para onde o clique leva (nome em `paginas`).
    pub leva_a: Option<String>,
    /// Um download ao clicar: (nome do arquivo, bytes).
    pub baixa: Option<(String, Vec<u8>)>,
    /// O script do navegador barra o clique (simula a interceptação em captura).
    pub barrado_no_navegador: Option<Bloqueio>,
}

impl Elemento {
    /// Um elemento comum, com o texto visível.
    #[must_use]
    pub fn com_texto(texto: &str) -> Self {
        Self {
            candidato: Candidato::com(&[("texto", texto)]),
            leva_a: None,
            baixa: None,
            barrado_no_navegador: None,
        }
    }
}

/// O site falso em memória.
#[derive(Debug, Default)]
pub struct NavegadorFalso {
    pub paginas: HashMap<String, Pagina>,
    pub elementos: HashMap<String, Elemento>,
    /// URL → nome da página.
    pub rotas: HashMap<String, String>,
    pub tabelas: HashMap<String, Vec<Vec<String>>>,
    pub atual: String,
    /// O que aconteceu, em ordem (sem valores).
    pub chamadas: Vec<String>,
    /// Segredos digitados (só testes).
    pub digitados: Vec<(String, String)>,
    /// Seletores que falham ao clicar (o site mudou).
    pub quebrados: Vec<String>,
    /// Quando acende, a página "mfa" passa a valer como "painel" (o usuário resolveu o MFA).
    pub liberacao: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pendentes: Vec<Bloqueio>,
}

impl NavegadorFalso {
    fn efetiva(&self) -> &str {
        let liberado = self
            .liberacao
            .as_ref()
            .is_some_and(|l| l.load(std::sync::atomic::Ordering::SeqCst));
        if liberado && self.atual == "mfa" {
            "painel"
        } else {
            &self.atual
        }
    }

    /// Define uma página.
    pub fn pagina_nomeada(&mut self, nome: &str, pagina: Pagina) {
        self.paginas.insert(nome.to_owned(), pagina);
    }
}

impl Navegador for NavegadorFalso {
    fn navegar(&mut self, url: &str) -> Resultado<()> {
        self.chamadas.push(format!("navegar {url}"));
        let nome = self
            .rotas
            .get(url)
            .ok_or_else(|| ErroDeRoteiro::Navegador("rota desconhecida".into()))?;
        self.atual = nome.clone();
        Ok(())
    }

    fn visivel(&mut self, seletor: &str) -> Resultado<bool> {
        let na_pagina = self.paginas.get(self.efetiva()).is_some_and(|p| {
            p.elementos.iter().any(|e| e.seletor == seletor)
                || p.formularios
                    .iter()
                    .any(|f| f.seletor == seletor || f.campos.iter().any(|c| c.seletor == seletor))
                || p.tabelas.iter().any(|t| t.seletor == seletor)
        });
        Ok(na_pagina && !self.quebrados.iter().any(|q| q == seletor))
    }

    fn descrever(&mut self, seletor: &str) -> Resultado<Option<Candidato>> {
        Ok(self.elementos.get(seletor).map(|e| e.candidato.clone()))
    }

    fn clicar(&mut self, seletor: &str) -> Resultado<()> {
        self.chamadas.push(format!("clicar {seletor}"));
        let Some(e) = self.elementos.get(seletor).cloned() else {
            return Err(ErroDeRoteiro::Navegador("elemento não existe".into()));
        };
        if let Some(b) = e.barrado_no_navegador {
            self.pendentes.push(b);
            return Ok(());
        }
        if let Some(p) = e.leva_a {
            self.atual = p;
        }
        Ok(())
    }

    fn preencher(&mut self, seletor: &str, texto: &str) -> Resultado<()> {
        self.chamadas.push(format!("preencher {seletor}"));
        self.digitados.push((seletor.to_owned(), texto.to_owned()));
        Ok(())
    }

    fn preencher_segredo(&mut self, seletor: &str, segredo: &Segredo) -> Resultado<()> {
        self.chamadas.push(format!("preencher_segredo {seletor}"));
        self.digitados
            .push((seletor.to_owned(), segredo.expor().to_owned()));
        Ok(())
    }

    fn clicar_e_baixar(
        &mut self,
        seletor: &str,
        pasta: &Path,
        _prazo: Duration,
    ) -> Resultado<PathBuf> {
        self.chamadas.push(format!("baixar {seletor}"));
        let e = self
            .elementos
            .get(seletor)
            .cloned()
            .ok_or_else(|| ErroDeRoteiro::Navegador("elemento não existe".into()))?;
        if let Some(b) = e.barrado_no_navegador {
            self.pendentes.push(b);
            return Err(ErroDeRoteiro::Navegador("clique barrado".into()));
        }
        let (nome, bytes) = e
            .baixa
            .ok_or_else(|| ErroDeRoteiro::Navegador("o clique não baixou nada".into()))?;
        let caminho = pasta.join(nome);
        std::fs::write(&caminho, bytes)?;
        Ok(caminho)
    }

    fn ler_tabela(&mut self, seletor: &str) -> Resultado<Vec<Vec<String>>> {
        self.chamadas.push(format!("ler_tabela {seletor}"));
        self.tabelas
            .get(seletor)
            .cloned()
            .ok_or_else(|| ErroDeRoteiro::Navegador("tabela não existe".into()))
    }

    fn pagina(&mut self) -> Resultado<Pagina> {
        Ok(self
            .paginas
            .get(self.efetiva())
            .cloned()
            .unwrap_or_default())
    }

    fn bloqueios(&mut self) -> Resultado<Vec<Bloqueio>> {
        Ok(std::mem::take(&mut self.pendentes))
    }
}
