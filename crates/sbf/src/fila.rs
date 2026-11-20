//! A Caixa de entrada real: a fila de reconhecimento do `inventario.sqlite` do cofre
//! (`sbf_inventario::caixa_de_entrada`) atrás da costura [`FonteDaFila`] da janela. As
//! sugestões do cartão são recalculadas na hora pelo `ReconhecedorDoCofre` sobre a cópia do
//! acervo, sem IA e sem mexer na fila; "Reconhecer de novo" é o mesmo reconhecimento, que
//! desta vez atualiza a linha.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_chaveiro::Chaveiro;
use sbf_inventario::caixa_de_entrada::{self, Contagens, Entrada, Filtro, Origem, Situacao};
use sbf_janela::caixa_de_entrada::FonteDaFila;
use sbf_reconhecimento::{ReconhecedorDoCofre, Resultado};

/// A fila do cofre.
pub struct FilaDoCofre {
    cofre: PathBuf,
    chaveiro: Option<Chaveiro>,
    /// Os reconhecedores nascem na primeira vez (abrir o catálogo de regras e o executor de
    /// scripts custa) e ficam: um para recalcular o cartão, outro para reconhecer de novo.
    para_o_cartao: Mutex<Option<ReconhecedorDoCofre>>,
    para_reconhecer: Mutex<Option<ReconhecedorDoCofre>>,
}

impl std::fmt::Debug for FilaDoCofre {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilaDoCofre")
            .field("cofre", &self.cofre)
            .finish_non_exhaustive()
    }
}

fn motivo(e: impl std::fmt::Display) -> String {
    format!("a Caixa de entrada não leu o inventário: {e}")
}

impl FilaDoCofre {
    #[must_use]
    pub fn nova(cofre: PathBuf, chaveiro: Option<Chaveiro>) -> Self {
        Self {
            cofre,
            chaveiro,
            para_o_cartao: Mutex::new(None),
            para_reconhecer: Mutex::new(None),
        }
    }

    fn conexao(&self) -> Result<Connection, String> {
        sbf_worker::banco::abrir(&self.cofre.join("inventario.sqlite")).map_err(motivo)
    }

    fn reconhecer(
        &self,
        guardado: &Mutex<Option<ReconhecedorDoCofre>>,
        com_fila: bool,
        sha256: &str,
    ) -> Result<Resultado, String> {
        let mut g = guardado
            .lock()
            .map_err(|_| "o reconhecimento travou".to_owned())?;
        if g.is_none() {
            let r = crate::reconhecer::montar(&self.cofre, self.chaveiro.clone(), false)?;
            *g = Some(if com_fila {
                r.com_fila(Origem::Arrastado)
            } else {
                r
            });
        }
        let Some(r) = g.as_ref() else {
            return Err("o reconhecimento não montou".to_owned());
        };
        r.reconhecer_conteudo(sha256, &mut |_| {}, &AtomicBool::new(false))
            .map_err(|e| e.to_string())
    }
}

impl FonteDaFila for FilaDoCofre {
    fn listar(&self, filtro: &Filtro) -> Result<Vec<Entrada>, String> {
        caixa_de_entrada::listar(&self.conexao()?, filtro).map_err(motivo)
    }

    fn contagens(&self) -> Result<Contagens, String> {
        caixa_de_entrada::contagens(&self.conexao()?).map_err(motivo)
    }

    fn definir_situacao(&self, sha256: &str, situacao: Situacao) -> Result<(), String> {
        match caixa_de_entrada::definir_situacao(&self.conexao()?, sha256, situacao) {
            Ok(true) => Ok(()),
            Ok(false) => Err("o item não está na Caixa de entrada".to_owned()),
            Err(e) => Err(motivo(e)),
        }
    }

    fn cartao(&self, sha256: &str) -> Result<Resultado, String> {
        self.reconhecer(&self.para_o_cartao, false, sha256)
    }

    fn reconhecer_de_novo(&self, sha256: &str) -> Result<Resultado, String> {
        self.reconhecer(&self.para_reconhecer, true, sha256)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_inventario::banco::Inventario;
    use sbf_reconhecimento::{Reconhecedor as _, Reconhecido};

    const CONTRACHEQUE: &str = include_str!("../../sbf-planilha/tests/contracheque-ficticio.txt");

    fn cofre() -> (tempfile::TempDir, PathBuf) {
        let raiz = tempfile::tempdir().expect("raiz");
        let cofre = raiz.path().join("cofre");
        std::fs::create_dir_all(&cofre).expect("cofre");
        let mut inv = Inventario::abrir(cofre.join("inventario.sqlite")).expect("abrir");
        inv.migrar(raiz.path().join("copia.sqlite"))
            .expect("migrar");
        (raiz, cofre)
    }

    #[test]
    fn a_fila_real_lista_recalcula_e_resolve() {
        let (raiz, cofre) = cofre();
        let arquivo = raiz.path().join("contracheque.txt");
        std::fs::write(&arquivo, CONTRACHEQUE).expect("escrever");
        // O CLI e o worker registram assim.
        let r = crate::reconhecer::montar(&cofre, None, false)
            .expect("montar")
            .com_fila(Origem::Cli);
        let visto = r
            .reconhecer(&arquivo, &mut |_| {}, &AtomicBool::new(false))
            .expect("reconhecer");
        std::fs::remove_file(&arquivo).expect("o original some");

        let fila = FilaDoCofre::nova(cofre, None);
        let lista = fila.listar(&Filtro::caixa()).expect("listar");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].origem, Origem::Cli);
        assert_eq!(fila.contagens().expect("contagens").reconhecidos, 1);

        // O cartão vem do acervo, sem o original, e sem mexer na fila.
        let cartao = fila.cartao(&visto.sha256).expect("cartão");
        assert!(matches!(
            cartao.reconhecido,
            Reconhecido::Contracheque { .. }
        ));
        assert_eq!(fila.listar(&Filtro::caixa()).expect("listar")[0].vezes, 1);

        // Reconhecer de novo atualiza a linha.
        fila.reconhecer_de_novo(&visto.sha256).expect("de novo");
        assert_eq!(fila.listar(&Filtro::caixa()).expect("listar")[0].vezes, 2);

        fila.definir_situacao(&visto.sha256, Situacao::Resolvida)
            .expect("resolver");
        assert!(fila.listar(&Filtro::caixa()).expect("listar").is_empty());
        assert!(
            fila.definir_situacao("inexistente", Situacao::Resolvida)
                .is_err()
        );
    }
}
