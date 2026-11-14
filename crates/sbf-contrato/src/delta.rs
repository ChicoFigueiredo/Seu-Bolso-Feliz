//! O delta por cursor (spec 08): `GET /api/v2/livro/{colecao}?desde=<cursor>` devolve só
//! o que mudou depois do cursor e o que foi apagado, mais o cursor novo.

use serde::{Deserialize, Serialize};

use crate::{Id, LinhasDaColecao};

/// O cursor é **opaco** para a estação: guarda-se o texto que o servidor devolveu e o
/// devolve em `?desde=`. Por dentro: o maior `updated_at` visto, o desempate por `id` e a
/// posição no registro de exclusões.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cursor(pub String);

/// O corpo de `GET /livro/{colecao}` com `?desde=`, e também **sem** `desde` na v2 (a
/// primeira carga é um delta a partir do zero, com `completo: true`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeltaDaColecao {
    /// Guardar; mandar de volta em `?desde=` na próxima vez.
    pub cursor: Cursor,
    /// `true`: `linhas` é a coleção inteira (a estação a troca por completo). `false`: são
    /// só as linhas novas ou mudadas, que entram por cima do que há no cache.
    pub completo: bool,
    /// As linhas novas ou mudadas desde o cursor.
    pub linhas: LinhasDaColecao,
    /// Os ids apagados desde o cursor (a estação os tira do cache).
    pub excluidas: Vec<Id>,
}
