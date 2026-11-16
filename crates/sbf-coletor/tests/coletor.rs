//! O que costura as coletas: o comparador (só leitura), a simulação da Pluggy, a agenda e o
//! dono. Gmail e Pluggy falsos locais; Postgres de teste com `SBF_PG_TESTE=1`.

mod comum;

use std::sync::Arc;
use std::time::Duration;

use comum::{banco, gmail_falso, novo_dono};
use sbf_coletor::agenda::{Coletor, ConfigDaAgenda, executar};
use sbf_coletor::comparador::comparar;
use sbf_coletor::dono::resolver_dono;
use sbf_coletor::gmail::varredura::{OpcoesDaVarredura, varrer};
use sbf_servidor::armazenamento::ArmazenamentoFalso;
use tokio::sync::watch;
use uuid::Uuid;

#[tokio::test]
async fn dono_pelo_banco_e_pelo_substituto_validado() {
    let Some(banco) = banco().await else { return };
    let dono = novo_dono(&banco).await;
    let c = banco.get().await.expect("conexão");
    // O substituto existente vale; o inexistente falha na largada, sem gravar nada.
    assert_eq!(
        resolver_dono(&**c, Some(&dono.to_string()))
            .await
            .expect("dono"),
        dono
    );
    let erro = resolver_dono(&**c, Some(&Uuid::new_v4().to_string()))
        .await
        .expect_err("inexistente")
        .to_string();
    assert!(erro.contains("LOCAL_USER_ID"), "{erro}");
    assert!(resolver_dono(&**c, Some("nao-e-uuid")).await.is_err());
    // Sem substituto: o dono do banco (a linha `owner` mais antiga).
    assert!(resolver_dono(&**c, None).await.is_ok());
    assert!(resolver_dono(&**c, Some("  ")).await.is_ok());
}

#[tokio::test]
async fn comparador_separa_bun_de_rust_e_so_le() {
    let Some(banco) = banco().await else { return };
    let dono = novo_dono(&banco).await;
    let (_falso, gmail) = gmail_falso().await;
    let armazenamento = ArmazenamentoFalso::novo();
    varrer(
        &gmail,
        &banco,
        &armazenamento,
        dono,
        &OpcoesDaVarredura::default(),
    )
    .await
    .expect("varredura do rust");
    // Uma execução "do Bun": sem `metadata.origem` e com o `stats` no formato do ScanStats.
    {
        let c = banco.get().await.expect("conexão");
        c.execute(
            "INSERT INTO public.ingestion_runs (user_id, source_type, status, metadata, stats) \
             VALUES ($1, 'gmail', 'completed', '{\"label\":\"Comprovantes\"}', \
             '{\"messagesListed\":3,\"attachmentsFound\":4,\"jobsCreated\":0,\"skippedAlreadyProcessed\":2,\"errors\":0}')",
            &[&dono],
        )
        .await
        .expect("execução do bun");
    }

    let r = comparar(&banco, dono, 8).await.expect("relatório");
    let rust = r
        .gmail_execucoes
        .iter()
        .find(|l| l.origem == "rust")
        .expect("linha do rust");
    let bun = r
        .gmail_execucoes
        .iter()
        .find(|l| l.origem == "bun")
        .expect("linha do bun");
    assert_eq!(
        (rust.anexos_encontrados, rust.jobs_criados, rust.mensagens),
        (3, 2, 3)
    );
    assert_eq!(
        (bun.anexos_encontrados, bun.jobs_criados, bun.ja_processados),
        (4, 0, 2)
    );
    assert_eq!(
        r.divergencias_do_gmail().len(),
        1,
        "3 contra 4 anexos achados no mesmo dia"
    );
    assert_eq!((r.gmail_documentos, r.gmail_documentos_sem_job), (2, 0));
    assert_eq!(r.pluggy_drafts, 0);
    assert_eq!(r.checkpoints.len(), 1);
    let texto = r.to_string();
    assert!(texto.contains("DIVERGE em"), "{texto}");

    // Só lê: a contagem de tudo é a mesma depois do relatório.
    let antes = contagens(&banco, dono).await;
    comparar(&banco, dono, 8).await.expect("de novo");
    assert_eq!(contagens(&banco, dono).await, antes);

    // E o banco recusa escrita dentro da transação do comparador.
    let mut c = banco.get().await.expect("conexão");
    let tx = c
        .build_transaction()
        .read_only(true)
        .start()
        .await
        .expect("tx");
    assert!(
        tx.execute(
            "DELETE FROM public.ingestion_runs WHERE user_id = $1",
            &[&dono]
        )
        .await
        .is_err()
    );
}

async fn contagens(banco: &deadpool_postgres::Pool, dono: Uuid) -> (i64, i64, i64, i64) {
    let c = banco.get().await.expect("conexão");
    let l = c
        .query_one(
            "SELECT (SELECT count(*) FROM public.ingestion_runs WHERE user_id = $1), \
             (SELECT count(*) FROM public.source_documents WHERE user_id = $1), \
             (SELECT count(*) FROM public.ingestion_jobs WHERE user_id = $1), \
             (SELECT count(*) FROM public.ingestion_checkpoints WHERE user_id = $1)",
            &[&dono],
        )
        .await
        .expect("contagens");
    (l.get(0), l.get(1), l.get(2), l.get(3))
}

#[tokio::test]
async fn agenda_roda_varios_ciclos_e_para_no_pedido() {
    let Some(banco) = banco().await else { return };
    let dono = novo_dono(&banco).await;
    let (falso, gmail) = gmail_falso().await;
    let armazenamento = Arc::new(ArmazenamentoFalso::novo());
    let coletor = Coletor {
        banco: banco.clone(),
        armazenamento: armazenamento.clone(),
        dono,
        gmail: Some(gmail),
        pluggy: None,
    };
    let agenda = ConfigDaAgenda {
        gmail: Some(Duration::from_millis(80)),
        pluggy: None,
        completa_a_cada: Duration::from_secs(3600),
        rotulo: "Comprovantes".into(),
        consulta: String::new(),
    };
    let (parar, ouvir) = watch::channel(false);
    let tarefa = tokio::spawn(async move {
        executar(&coletor, &agenda, ouvir).await;
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    parar.send(true).expect("parar");
    tokio::time::timeout(Duration::from_secs(5), tarefa)
        .await
        .expect("a agenda parou")
        .expect("sem pânico");

    let listas: Vec<_> = falso
        .pedidos()
        .into_iter()
        .filter(|p| p.caminho == "/messages" && p.parametro("pageToken").is_none())
        .collect();
    assert!(listas.len() >= 2, "ao menos dois ciclos: {}", listas.len());
    assert!(
        listas[0].parametro("q").is_none(),
        "o primeiro ciclo é completo, sem after:"
    );
    assert!(
        listas[1]
            .parametro("q")
            .is_some_and(|q| q.starts_with("after%3A")),
        "os seguintes são incrementais"
    );
    assert_eq!(
        armazenamento.gravacoes(),
        2,
        "os ciclos repetidos não regravam nada"
    );
    let c = banco.get().await.expect("conexão");
    let n: i64 = c
        .query_one(
            "SELECT count(*) FROM public.ingestion_jobs WHERE user_id = $1",
            &[&dono],
        )
        .await
        .expect("jobs")
        .get(0);
    assert_eq!(n, 2);
}
