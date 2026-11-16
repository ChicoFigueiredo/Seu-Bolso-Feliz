//! O delta por cursor (spec 08): `GET /api/v2/livro/{colecao}?desde=<cursor>`.
//!
//! O cursor é opaco para a estação. Por dentro, JSON em base64url: `v` e `i` são o maior par
//! `(updated_at em microssegundos, id)` já entregue, e `x` é o maior `seq` de `exclusoes` já
//! entregue (migração `0005_versoes.sql`). Um cursor ausente é o zero: devolve a coleção
//! inteira, com `completo: true`.
//!
//! Transações longas: uma linha gravada por uma transação que começou **antes** e terminou
//! **depois** da leitura que produziu o cursor tem `updated_at` menor que o cursor e ficaria
//! para sempre fora do delta. Duas defesas: (1) o delta lê as exclusões e as linhas em uma
//! transação `REPEATABLE READ` (uma foto só); (2) o cursor recua [`MARGEM_DO_CURSOR_US`] em
//! relação ao relógio **do banco** na hora da leitura, então o delta seguinte devolve de novo
//! o que mudou nessa margem (a estação aplica linhas repetidas sem dano) e cobre transações
//! de até essa duração. O que passar disso é curado pela carga completa (cursor ausente), que
//! a estação pede periodicamente (`sbf_estacao::sincronizacao`).

/// Quanto o cursor recua atrás do relógio do banco: 5 segundos, em microssegundos.
pub const MARGEM_DO_CURSOR_US: i64 = 5_000_000;

use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sbf_contrato::{Colecao, Cursor, DeltaDaColecao, LinhasDaColecao, Versao};
use serde::{Deserialize, Serialize};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::EstadoDoServidor;
use crate::estacoes::EstacaoAutenticada;
use crate::http::{ContratoDoPedido, ErroHttp};
use crate::livro::{ler_colecao, ler_rateios_depois};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Posicao {
    v: i64,
    i: Uuid,
    x: i64,
}

impl Posicao {
    const ZERO: Posicao = Posicao {
        v: 0,
        i: Uuid::nil(),
        x: 0,
    };

    fn codificar(self) -> Cursor {
        let json = serde_json::to_vec(&self).unwrap_or_default();
        Cursor(URL_SAFE_NO_PAD.encode(json))
    }

    fn decodificar(texto: &str) -> Result<Self, ErroHttp> {
        let invalido = || ErroHttp::pedido_invalido("o cursor não é um cursor do servidor");
        let bytes = URL_SAFE_NO_PAD.decode(texto).map_err(|_| invalido())?;
        serde_json::from_slice(&bytes).map_err(|_| invalido())
    }
}

#[derive(Debug, Deserialize)]
pub struct Parametros {
    pub desde: Option<String>,
}

fn micros(versao: Option<&Versao>) -> i64 {
    versao.and_then(|v| v.0.parse().ok()).unwrap_or(0)
}

/// As linhas com chave maior que `depois`, na ordem da chave, e a maior chave entregue.
fn depois_de<T>(
    linhas: Vec<T>,
    chave: impl Fn(&T) -> (i64, Uuid),
    depois: (i64, Uuid),
) -> (Vec<T>, Option<(i64, Uuid)>) {
    let mut mantidas: Vec<((i64, Uuid), T)> = linhas
        .into_iter()
        .map(|l| (chave(&l), l))
        .filter(|(k, _)| *k > depois)
        .collect();
    mantidas.sort_by_key(|(k, _)| *k);
    let maior = mantidas.last().map(|(k, _)| *k);
    (mantidas.into_iter().map(|(_, l)| l).collect(), maior)
}

/// `GET /livro/{colecao}` na v2.
pub async fn ver(
    State(estado): State<EstadoDoServidor>,
    quem: EstacaoAutenticada,
    Extension(contrato): Extension<ContratoDoPedido>,
    Path((_versao, colecao)): Path<(String, String)>,
    Query(parametros): Query<Parametros>,
) -> Result<Json<DeltaDaColecao>, ErroHttp> {
    let colecao = Colecao::do_caminho(&colecao)
        .filter(|c| c.desde_a_versao() <= contrato.0)
        .ok_or_else(|| ErroHttp::nao_encontrado("coleção desconhecida"))?;
    let desde = match parametros.desde.as_deref().filter(|d| !d.is_empty()) {
        Some(texto) => Some(Posicao::decodificar(texto)?),
        None => None,
    };
    let mut cliente = estado.banco.get().await?;
    // Uma foto só para as exclusões e as linhas.
    let foto = cliente
        .build_transaction()
        .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .await?;
    let delta = calcular(&*foto, colecao, quem.dono, desde).await?;
    foto.commit().await?;
    Ok(Json(delta))
}

async fn calcular<C: GenericClient>(
    cliente: &C,
    colecao: Colecao,
    dono: Uuid,
    desde: Option<Posicao>,
) -> Result<DeltaDaColecao, ErroHttp> {
    let base = desde.unwrap_or(Posicao::ZERO);
    // O relógio do banco na foto menos a margem: o piso do próximo cursor.
    let agora_us: i64 = cliente
        .query_one("SELECT (extract(epoch FROM now()) * 1000000)::bigint", &[])
        .await?
        .try_get(0)?;
    let piso = (agora_us - MARGEM_DO_CURSOR_US).max(base.v);
    let depois = (base.v, base.i);
    // O maior `seq` das exclusões vem antes das linhas: uma exclusão que chegue entre as
    // duas leituras aparece de novo no delta seguinte (a estação a aplica de novo, sem dano).
    let apagadas = cliente
        .query(
            "SELECT id, seq FROM public.exclusoes WHERE user_id = $1 AND colecao = $2 AND seq > $3 ORDER BY seq",
            &[&dono, &colecao.caminho(), &base.x],
        )
        .await?;
    let mut excluidas = Vec::with_capacity(apagadas.len());
    let mut maior_seq = base.x;
    for linha in &apagadas {
        excluidas.push(linha.try_get::<_, Uuid>("id")?);
        maior_seq = maior_seq.max(linha.try_get::<_, i64>("seq")?);
    }

    let linhas = if colecao == Colecao::Rateios {
        let achadas = ler_rateios_depois(cliente, dono, depois).await?;
        LinhasDaColecao::Rateios(achadas.into_iter().map(|(_, r)| r).collect())
    } else {
        filtrar(ler_colecao(cliente, colecao, dono).await?, depois).0
    };
    // O cursor anda até o piso (relógio do banco menos a margem), não até a maior linha: o
    // que mudou depois do piso volta no próximo delta. Numa leitura sem cursor, também.
    Ok(DeltaDaColecao {
        cursor: Posicao {
            v: piso,
            i: Uuid::nil(),
            x: maior_seq,
        }
        .codificar(),
        completo: desde.is_none(),
        linhas,
        excluidas,
    })
}

fn filtrar(linhas: LinhasDaColecao, depois: (i64, Uuid)) -> (LinhasDaColecao, Option<(i64, Uuid)>) {
    use LinhasDaColecao as L;
    macro_rules! comum {
        ($v:expr, $variante:ident) => {{
            let (l, m) = depois_de($v, |x| (micros(Some(&x.versao)), x.id), depois);
            (L::$variante(l), m)
        }};
    }
    match linhas {
        L::Contas(v) => comum!(v, Contas),
        L::Instituicoes(v) => comum!(v, Instituicoes),
        L::Cartoes(v) => comum!(v, Cartoes),
        L::Faturas(v) => comum!(v, Faturas),
        L::ItensDeFatura(v) => comum!(v, ItensDeFatura),
        L::Movimentacoes(v) => comum!(v, Movimentacoes),
        L::Transferencias(v) => comum!(v, Transferencias),
        L::Categorias(v) => comum!(v, Categorias),
        L::Tags(v) => comum!(v, Tags),
        L::Centros(v) => comum!(v, Centros),
        L::Fornecedores(v) => comum!(v, Fornecedores),
        L::Apelidos(v) => comum!(v, Apelidos),
        L::Contratos(v) => comum!(v, Contratos),
        L::Dividas(v) => comum!(v, Dividas),
        L::Parcelas(v) => comum!(v, Parcelas),
        L::Recorrentes(v) => comum!(v, Recorrentes),
        L::Ocorrencias(v) => comum!(v, Ocorrencias),
        L::LotesDeRascunhos(v) => comum!(v, LotesDeRascunhos),
        L::Rascunhos(v) => comum!(v, Rascunhos),
        L::Documentos(v) => {
            let (l, m) = depois_de(v, |x| (micros(x.versao.as_ref()), x.id), depois);
            (L::Documentos(l), m)
        }
        // Os rateios não passam por aqui: a versão deles é a da movimentação.
        L::Rateios(v) => (L::Rateios(v), None),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cursor_ida_e_volta_e_lixo_e_pedido_invalido() {
        let p = Posicao {
            v: 1_763_049_600_000_000,
            i: Uuid::from_u128(7),
            x: 42,
        };
        assert_eq!(Posicao::decodificar(&p.codificar().0).unwrap(), p);
        assert!(Posicao::decodificar("isto não é base64!").is_err());
        assert!(Posicao::decodificar("e30").is_err()); // `{}`
    }

    #[test]
    fn depois_de_filtra_ordena_e_acha_o_maior() {
        let linhas = vec![(5, 1u128), (3, 9), (5, 0), (1, 1)];
        let (l, m) = depois_de(
            linhas,
            |(v, i)| (*v, Uuid::from_u128(*i)),
            (3, Uuid::from_u128(9)),
        );
        assert_eq!(l, vec![(5, 0), (5, 1)]);
        assert_eq!(m, Some((5, Uuid::from_u128(1))));
        let (vazio, nada) = depois_de(
            vec![(1, 1u128)],
            |(v, i)| (*v, Uuid::from_u128(*i)),
            (9, Uuid::nil()),
        );
        assert!(vazio.is_empty() && nada.is_none());
    }
}
