//! O contrato do navegador. O executor só conhece este trait; o Chrome por CDP
//! ([`crate::chrome::Chrome`]) e o navegador falso dos testes o implementam.

use crate::bloqueio::{Bloqueio, Candidato};
use crate::erro::Resultado;
use crate::pagina::Pagina;
use sbf_chaveiro::Segredo;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Um navegador controlável. Nenhum método devolve valor de campo.
pub trait Navegador {
    /// Abre a URL e espera carregar.
    fn navegar(&mut self, url: &str) -> Resultado<()>;
    /// O seletor existe e está visível agora?
    fn visivel(&mut self, seletor: &str) -> Resultado<bool>;
    /// Descreve o elemento para a lista de bloqueio; `None` se não existe.
    fn descrever(&mut self, seletor: &str) -> Resultado<Option<Candidato>>;
    /// Clica (clique de verdade no centro do elemento).
    fn clicar(&mut self, seletor: &str) -> Resultado<()>;
    /// Preenche com texto fixo.
    fn preencher(&mut self, seletor: &str, texto: &str) -> Resultado<()>;
    /// Preenche com um segredo (senha, CPF). O segredo só existe em memória, entre o chaveiro
    /// e o campo; a implementação não o copia para log, erro ou `Debug`.
    fn preencher_segredo(&mut self, seletor: &str, segredo: &Segredo) -> Resultado<()>;
    /// Clica e espera o arquivo baixado cair em `pasta`.
    fn clicar_e_baixar(&mut self, seletor: &str, pasta: &Path, prazo: Duration) -> Resultado<PathBuf>;
    /// As linhas da tabela, como texto.
    fn ler_tabela(&mut self, seletor: &str) -> Resultado<Vec<Vec<String>>>;
    /// A página simplificada (sem valores de campo).
    fn pagina(&mut self) -> Resultado<Pagina>;
    /// Os bloqueios que o script do navegador barrou desde a última chamada.
    fn bloqueios(&mut self) -> Resultado<Vec<Bloqueio>>;
}
