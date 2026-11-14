//! A entrega dos avisos: deduplica por destinatário (um alerta não repete até o estado
//! mudar), monta o texto no nível dele, manda e registra o que saiu.

use std::collections::BTreeMap;

use deadpool_postgres::Pool;
use sbf_livro::Data;

use crate::alertas::{Alerta, TipoDeAlerta};
use crate::erro::ErroDeCanal;
use crate::repo::{self, Destinatario};
use crate::telegram::ClienteTelegram;
use crate::texto::{mensagem_do_tipo, resumo_do_dia};

/// Manda um texto e registra a saída (destinatário, tipo, tamanho), entregue ou não.
pub async fn enviar_registrando(
    banco: &Pool,
    telegram: &ClienteTelegram,
    destinatario: &Destinatario,
    tipo: &str,
    texto: &str,
) -> Result<(), ErroDeCanal> {
    let resultado = telegram.enviar_texto(destinatario.chat, texto).await;
    // O registro não pode derrubar o aviso, nem o aviso o registro.
    let registro =
        repo::registrar_saida(banco, destinatario, tipo, texto.len(), resultado.is_ok()).await;
    resultado?;
    registro
}

/// Envia a um destinatário só os alertas novos (ou de estado mudado). Devolve quantos
/// alertas foram avisados.
pub async fn avisar(
    banco: &Pool,
    telegram: &ClienteTelegram,
    destinatario: &Destinatario,
    alertas: &[Alerta],
) -> Result<usize, ErroDeCanal> {
    let avisados = repo::estados_avisados(banco, destinatario.chat).await?;
    let mut por_tipo: BTreeMap<TipoDeAlerta, Vec<&Alerta>> = BTreeMap::new();
    for a in alertas {
        if avisados.get(&a.chave) != Some(&a.estado) {
            por_tipo.entry(a.tipo).or_default().push(a);
        }
    }
    let mut enviados = 0;
    for (tipo, grupo) in por_tipo {
        let texto = mensagem_do_tipo(tipo, &grupo, destinatario.nivel);
        // Falhou: não marca como avisado, a próxima rodada tenta de novo.
        if let Err(e) =
            enviar_registrando(banco, telegram, destinatario, tipo.codigo(), &texto).await
        {
            eprintln!("sbf canais: aviso de {} não saiu: {e}", tipo.codigo());
            continue;
        }
        for a in &grupo {
            repo::gravar_avisado(
                banco,
                destinatario.dono,
                destinatario.chat,
                &a.chave,
                &a.estado,
            )
            .await?;
        }
        enviados += grupo.len();
    }
    let chaves: Vec<String> = alertas.iter().map(|a| a.chave.clone()).collect();
    repo::esquecer_ausentes(banco, destinatario.chat, &chaves).await?;
    Ok(enviados)
}

/// O resumo diário: contagens e totais, sem nomes.
pub async fn resumir(
    banco: &Pool,
    telegram: &ClienteTelegram,
    destinatario: &Destinatario,
    hoje: Data,
    alertas: &[Alerta],
) -> Result<(), ErroDeCanal> {
    let texto = resumo_do_dia(hoje, alertas);
    enviar_registrando(banco, telegram, destinatario, "resumo_diario", &texto).await
}
