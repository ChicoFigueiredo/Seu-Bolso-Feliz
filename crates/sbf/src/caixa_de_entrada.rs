//! A Caixa de entrada real: a tabela `caixa_entrada` do `inventario.sqlite` do cofre
//! (`sbf_inventario::caixa_de_entrada`) atrás da costura [`FonteDaCaixaDeEntrada`] da janela.
//! O cartão é recalculado na hora pelo `ReconhecedorDoCofre` sobre a cópia do acervo, **só com
//! o texto já guardado** (sem IA, sem extrair, sem OCR: roda na thread da interface) e sem
//! mexer na Caixa de entrada. "Reconhecer de novo" extrai o que faltar (a janela o roda numa
//! thread à parte) e só atualiza a linha: não a cria e não reabre o que o usuário decidiu.
//! Resolver, dispensar e reabrir são decisões do usuário no log do inventário.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_chaveiro::Chaveiro;
use sbf_inventario::caixa_de_entrada::{self, Contagens, Entrada, Filtro, Situacao};
use sbf_janela::caixa_de_entrada::FonteDaCaixaDeEntrada;
use sbf_reconhecimento::{ReconhecedorDoCofre, Resultado};

/// A Caixa de entrada do cofre.
pub struct CaixaDeEntradaDoCofre {
    cofre: PathBuf,
    chaveiro: Option<Chaveiro>,
    /// Os reconhecedores nascem na primeira vez (abrir o catálogo de regras e o executor de
    /// scripts custa) e ficam: um para recalcular o cartão (só texto guardado), outro para
    /// reconhecer de novo (extrai e atualiza a linha).
    para_o_cartao: Mutex<Option<ReconhecedorDoCofre>>,
    para_reconhecer: Mutex<Option<ReconhecedorDoCofre>>,
}

impl std::fmt::Debug for CaixaDeEntradaDoCofre {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaixaDeEntradaDoCofre")
            .field("cofre", &self.cofre)
            .finish_non_exhaustive()
    }
}

fn motivo(e: impl std::fmt::Display) -> String {
    format!("a Caixa de entrada não leu o inventário: {e}")
}

impl CaixaDeEntradaDoCofre {
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
        de_novo: bool,
        sha256: &str,
    ) -> Result<Resultado, String> {
        let mut g = guardado
            .lock()
            .map_err(|_| "o reconhecimento travou".to_owned())?;
        if g.is_none() {
            let r = crate::reconhecer::montar(&self.cofre, self.chaveiro.clone(), false)?;
            *g = Some(if de_novo {
                r.atualizando_a_caixa_de_entrada()
            } else {
                r.so_com_texto_guardado()
            });
        }
        let Some(r) = g.as_ref() else {
            return Err("o reconhecimento não montou".to_owned());
        };
        r.reconhecer_conteudo(sha256, &mut |_| {}, &AtomicBool::new(false))
            .map_err(|e| e.to_string())
    }
}

impl FonteDaCaixaDeEntrada for CaixaDeEntradaDoCofre {
    fn listar(&self, filtro: &Filtro) -> Result<Vec<Entrada>, String> {
        caixa_de_entrada::listar(&self.conexao()?, filtro).map_err(motivo)
    }

    fn contagens(&self) -> Result<Contagens, String> {
        caixa_de_entrada::contagens(&self.conexao()?).map_err(motivo)
    }

    fn definir_situacao(&self, sha256: &str, situacao: Situacao) -> Result<(), String> {
        caixa_de_entrada::decidir(&self.conexao()?, sha256, situacao)
            .map(|_| ())
            .map_err(|e| e.to_string())
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
    use sbf_inventario::caixa_de_entrada::Canal;
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
    fn a_caixa_real_lista_recalcula_e_resolve_pelo_log() {
        let (raiz, cofre) = cofre();
        let arquivo = raiz.path().join("contracheque.txt");
        std::fs::write(&arquivo, CONTRACHEQUE).expect("escrever");
        // O CLI e o worker registram assim.
        let r = crate::reconhecer::montar(&cofre, None, false)
            .expect("montar")
            .com_caixa_de_entrada(Canal::Cli, true);
        let visto = r
            .reconhecer(&arquivo, &mut |_| {}, &AtomicBool::new(false))
            .expect("reconhecer");
        std::fs::remove_file(&arquivo).expect("o original some");

        let caixa = CaixaDeEntradaDoCofre::nova(cofre.clone(), None);
        let lista = caixa.listar(&Filtro::caixa()).expect("listar");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].canal, Canal::Cli);
        assert_eq!(caixa.contagens().expect("contagens").reconhecidos, 1);

        // O cartão vem do acervo, sem o original, e sem mexer na Caixa de entrada.
        let cartao = caixa.cartao(&visto.sha256).expect("cartão");
        assert!(matches!(
            cartao.reconhecido,
            Reconhecido::Contracheque { .. }
        ));
        assert_eq!(caixa.listar(&Filtro::caixa()).expect("listar")[0].vezes, 1);

        // Reconhecer de novo atualiza a linha.
        caixa.reconhecer_de_novo(&visto.sha256).expect("de novo");
        assert_eq!(caixa.listar(&Filtro::caixa()).expect("listar")[0].vezes, 2);

        // Resolver é uma decisão no log, e o Desfazer de sempre a desfaz.
        caixa
            .definir_situacao(&visto.sha256, Situacao::Resolvida)
            .expect("resolver");
        assert!(caixa.listar(&Filtro::caixa()).expect("listar").is_empty());
        let conn = caixa.conexao().expect("conexão");
        let decisoes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM decisao WHERE campo = 'caixa_entrada' AND autor = 'usuario'",
                [],
                |l| l.get(0),
            )
            .expect("contar");
        assert_eq!(decisoes, 1);
        drop(conn);
        // Reconhecer de novo não reabre o que foi resolvido.
        caixa.reconhecer_de_novo(&visto.sha256).expect("de novo");
        assert!(caixa.listar(&Filtro::caixa()).expect("listar").is_empty());
        assert!(
            caixa
                .definir_situacao("inexistente", Situacao::Resolvida)
                .is_err()
        );
    }
}
