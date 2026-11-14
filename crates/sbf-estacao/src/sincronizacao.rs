//! Uma troca com o servidor (spec 07, "O cache local do livro-caixa"; grill Q4, Q5, Q11):
//!
//! 1. `GET /estado` (o primeiro contato: estação revogada, contrato recusado ou sem rede
//!    param aqui);
//! 2. as alterações Aguardando vão, uma a uma, se o servidor não tem contrato mais novo que
//!    o desta estação (Q4: servidor mais novo, só leitura);
//! 3. as revogações pedidas nesta estação vão;
//! 4. cada coleção vem inteira se a `ETag` mudou (Q5); as que mudaram entram no cache numa
//!    transação só, junto com o `/estado` e a hora da cópia;
//! 5. a lista de estações.
//!
//! Estação revogada em qualquer passo (Q11): apaga a credencial e o `livro-caixa.sqlite`, e
//! avisa. O cofre e o inventário não são tocados (nem abertos).

use crate::alteracoes::aplicar_resposta;
use crate::cache::{CacheDoLivro, ErroDoCache, apagar_arquivos};
use crate::credencial::GuardaDeCredencial;
use crate::escrita::EstadoDaEscrita;
use crate::servidor::{ClienteDoServidor, ErroDoServidor};
use crate::texto::instante_iso;
use bancada_rede::Relogio;
use sbf_contrato::{CodigoDeErro, Colecao, Estacao, Estado, Id};
use sbf_inventario::alteracao::{EstadoDaAlteracao, Troca};
use std::cmp::Ordering;
use std::path::Path;

/// Como terminou uma troca.
#[derive(Debug, Clone, PartialEq)]
pub enum Desfecho {
    /// Terminou bem. `regras_mais_novas`: a versão das regras do servidor, quando é mais
    /// nova que a desta estação. `so_leitura`: o servidor tem contrato mais novo, e as
    /// alterações ficaram esperando.
    Sincronizado {
        copia_de: String,
        regras_mais_novas: Option<String>,
        so_leitura: bool,
    },
    /// Sem rede: a cópia anterior continua valendo, inteira.
    SemRede,
    /// A estação foi revogada: credencial e cache apagados.
    EstacaoRevogada,
    /// O servidor recusou o contrato desta estação: atualize o app.
    AtualizeOApp,
    /// Este app fala um contrato que o servidor ainda não conhece.
    ServidorDesatualizado,
    /// Sem credencial (nunca pareou), ou credencial que o servidor não conhece.
    NaoPareada,
    /// Outro erro, com a mensagem (sem segredo).
    Falhou(String),
}

/// Compara versões `1.2.3` por número (o que não é número compara como texto).
pub fn comparar_versoes(a: &str, b: &str) -> Ordering {
    let partes = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split(['.', '-', '+'])
            .map_while(|p| p.parse().ok())
            .collect()
    };
    let (pa, pb) = (partes(a), partes(b));
    if pa.is_empty() || pb.is_empty() {
        return a.cmp(b);
    }
    pa.cmp(&pb)
}

/// O que a troca precisa.
#[derive(Debug, Clone, Copy)]
pub struct Sincronizador<'a> {
    pub cliente: &'a ClienteDoServidor,
    pub guarda: &'a dyn GuardaDeCredencial,
    pub relogio: &'a dyn Relogio,
    pub versao_das_regras: &'a str,
}

#[derive(Debug, Default)]
struct Contas {
    colecoes: Vec<String>,
    linhas: i64,
    enviadas: i64,
    aceitas: i64,
    recusadas: i64,
}

enum Parada {
    Servidor(ErroDoServidor),
    Cache(ErroDoCache),
}

impl From<ErroDoServidor> for Parada {
    fn from(e: ErroDoServidor) -> Self {
        Self::Servidor(e)
    }
}

impl From<ErroDoCache> for Parada {
    fn from(e: ErroDoCache) -> Self {
        Self::Cache(e)
    }
}

impl Sincronizador<'_> {
    fn agora(&self) -> String {
        instante_iso(self.relogio.agora())
    }

    /// Faz uma troca completa sobre o cache em `caminho`. Bloqueia: roda fora da thread da
    /// interface.
    pub fn trocar(&self, caminho: &Path) -> Desfecho {
        let host = self.cliente.servidor().host().to_owned();
        if !self.guarda.tem(&host) {
            return Desfecho::NaoPareada;
        }
        let mut cache = match CacheDoLivro::abrir(caminho) {
            Ok(c) => c,
            Err(e) => return Desfecho::Falhou(e.to_string()),
        };
        let inicio = self.agora();
        let id_da_troca = cache.iniciar_troca(&inicio).ok();
        let mut contas = Contas::default();
        let resultado = self.passos(&mut cache, &mut contas);
        let desfecho = match resultado {
            Ok(d) => d,
            Err(Parada::Cache(e)) => Desfecho::Falhou(e.to_string()),
            Err(Parada::Servidor(e)) => match e {
                ErroDoServidor::EstacaoRevogada => Desfecho::EstacaoRevogada,
                ErroDoServidor::SemRede(_) => Desfecho::SemRede,
                ErroDoServidor::ContratoAntigo => Desfecho::AtualizeOApp,
                ErroDoServidor::ContratoMaisNovo => Desfecho::ServidorDesatualizado,
                ErroDoServidor::NaoAutenticado => Desfecho::NaoPareada,
                outro => Desfecho::Falhou(outro.to_string()),
            },
        };
        if desfecho == Desfecho::EstacaoRevogada {
            drop(cache);
            revogar_localmente(self.guarda, &host, caminho);
            return desfecho;
        }
        if let Some(id) = id_da_troca {
            let erro = match &desfecho {
                Desfecho::Sincronizado { .. } => None,
                Desfecho::SemRede => Some("sem rede".to_owned()),
                Desfecho::AtualizeOApp => Some("o servidor pede um app mais novo".to_owned()),
                Desfecho::ServidorDesatualizado => {
                    Some("o servidor ainda não conhece este contrato".to_owned())
                }
                Desfecho::NaoPareada => Some("o servidor não reconhece esta estação".to_owned()),
                Desfecho::Falhou(m) => Some(m.clone()),
                Desfecho::EstacaoRevogada => None,
            };
            let _ = cache.terminar_troca(
                id,
                &Troca {
                    inicio,
                    fim: Some(self.agora()),
                    colecoes: contas.colecoes,
                    linhas: contas.linhas,
                    enviadas: contas.enviadas,
                    aceitas: contas.aceitas,
                    recusadas: contas.recusadas,
                    erro,
                },
            );
        }
        desfecho
    }

    fn passos(&self, cache: &mut CacheDoLivro, contas: &mut Contas) -> Result<Desfecho, Parada> {
        let estado: Estado = self.cliente.estado()?;
        if estado.estacao.revogada {
            return Err(Parada::Servidor(ErroDoServidor::EstacaoRevogada));
        }
        let anterior = cache.estado_do_servidor()?;
        let outra_estacao = anterior
            .as_ref()
            .is_some_and(|a| a.estacao.id != estado.estacao.id);
        let so_leitura = estado.contrato > self.cliente.servidor().versao_do_contrato();

        // As alterações só vão se a fila é desta mesma estação e o servidor não é mais novo.
        let v2 = self.cliente.servidor().versao_do_contrato() >= 2;
        if !so_leitura && !outra_estacao {
            self.enviar_alteracoes(cache, contas)?;
            if v2 {
                self.enviar_escritas(cache, contas)?;
            }
        }
        for estacao in cache.revogacoes_pedidas()? {
            if let Ok(id) = estacao.parse::<Id>() {
                match self.cliente.revogar(id) {
                    Ok(_) | Err(ErroDoServidor::Api { status: 404, .. }) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            cache.esquecer_revogacao(&estacao)?;
        }

        let copia_de;
        if v2 {
            // Contrato v2: o que mudou desde o cursor de cada coleção (sem cursor, a coleção
            // inteira).
            let cursores = if outra_estacao {
                Default::default()
            } else {
                cache.cursores()?
            };
            let mut deltas = Vec::with_capacity(Colecao::TODAS.len());
            for colecao in Colecao::TODAS {
                let delta = self.cliente.delta(colecao, cursores.get(&colecao))?;
                if delta.completo || delta.linhas.quantas() > 0 || !delta.excluidas.is_empty() {
                    contas.colecoes.push(colecao.caminho().to_owned());
                }
                deltas.push(delta);
            }
            copia_de = self.agora();
            let linhas = cache.trocar_com_deltas(&estado, &deltas, &copia_de)?;
            contas.linhas = i64::try_from(linhas).unwrap_or(i64::MAX);
            // O que o usuário fez e o servidor ainda não aceitou continua visível.
            cache
                .reaplicar_escritas()
                .map_err(|e| Parada::Cache(ErroDoCache(e.to_string())))?;
        } else {
            let etags = if outra_estacao {
                Default::default()
            } else {
                cache.etags()?
            };
            let versao = self.cliente.servidor().versao_do_contrato();
            let mut novas = Vec::new();
            for colecao in Colecao::TODAS
                .into_iter()
                .filter(|c| c.desde_a_versao() <= versao)
            {
                if let Some(nova) = self
                    .cliente
                    .colecao(colecao, etags.get(&colecao).map(String::as_str))?
                {
                    contas.colecoes.push(colecao.caminho().to_owned());
                    novas.push((nova.linhas, nova.etag));
                }
            }
            copia_de = self.agora();
            let linhas = cache.trocar(&estado, &novas, &copia_de)?;
            contas.linhas = i64::try_from(linhas).unwrap_or(i64::MAX);
        }

        let estacoes = self.cliente.estacoes()?;
        cache.gravar_estacoes(&estacoes)?;

        let regras_mais_novas = (comparar_versoes(&estado.regras, self.versao_das_regras)
            == Ordering::Greater)
            .then(|| estado.regras.clone());
        Ok(Desfecho::Sincronizado {
            copia_de,
            regras_mais_novas,
            so_leitura,
        })
    }

    /// As escritas da fila nova (contrato v2), da mais antiga para a mais nova. A rede que cai
    /// para a troca no ponto em que estava: o que não foi respondido continua Aguardando e
    /// vai de novo com o mesmo id.
    fn enviar_escritas(&self, cache: &mut CacheDoLivro, contas: &mut Contas) -> Result<(), Parada> {
        let fila: Vec<Id> = cache
            .escritas()?
            .into_iter()
            .filter(|e| e.estado == EstadoDaEscrita::Aguardando)
            .map(|e| e.id)
            .collect();
        for id in fila {
            // Relê: o aceite da anterior pode ter mudado a versão de base desta.
            let Some(escrita) = cache
                .escrita(id)?
                .filter(|e| e.estado == EstadoDaEscrita::Aguardando)
            else {
                continue;
            };
            let agora = self.agora();
            match self.cliente.propor(&escrita.proposta) {
                Ok(resposta) => {
                    contas.enviadas += 1;
                    match cache.aplicar_resposta_da_escrita(escrita.id, resposta, &agora) {
                        Ok(true) => contas.aceitas += 1,
                        Ok(false) => contas.recusadas += 1,
                        Err(e) => return Err(Parada::Cache(ErroDoCache(e.to_string()))),
                    }
                }
                // Servidor que ainda não conhece a Ação: fica Aguardando.
                Err(ErroDoServidor::Api {
                    codigo: Some(CodigoDeErro::AcaoAindaNaoAceita),
                    ..
                }) => {}
                // Pedido que o servidor considera inválido: recusa local, para o usuário ver.
                Err(ErroDoServidor::Api {
                    status: 422 | 413,
                    mensagem,
                    ..
                }) => {
                    contas.enviadas += 1;
                    contas.recusadas += 1;
                    cache
                        .recusar_localmente(escrita.id, &mensagem, &agora)
                        .map_err(|e| Parada::Cache(ErroDoCache(e.to_string())))?;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn enviar_alteracoes(
        &self,
        cache: &mut CacheDoLivro,
        contas: &mut Contas,
    ) -> Result<(), Parada> {
        let mut fila: Vec<_> = cache
            .alteracoes()?
            .into_iter()
            .filter(|r| r.alteracao.estado == EstadoDaAlteracao::Aguardando)
            .collect();
        // A mais antiga primeiro.
        fila.reverse();
        for registro in fila {
            let resposta = match self.cliente.propor(&registro.proposta) {
                Ok(r) => r,
                // A ação ainda não é aceita pelo servidor: fica Aguardando.
                Err(ErroDoServidor::Api { status: 422, .. }) => continue,
                Err(e) => return Err(e.into()),
            };
            contas.enviadas += 1;
            let agora = self.agora();
            match aplicar_resposta(cache, registro, resposta, &agora) {
                Ok(true) => contas.aceitas += 1,
                Ok(false) => contas.recusadas += 1,
                Err(e) => return Err(Parada::Cache(ErroDoCache(e.to_string()))),
            }
        }
        Ok(())
    }
}

/// Q11: apaga a credencial e o cache. O cofre e o inventário ficam.
pub fn revogar_localmente(guarda: &dyn GuardaDeCredencial, host: &str, caminho: &Path) {
    let _ = guarda.apagar(host);
    // No Windows o arquivo pode estar aberto pela janela: a janela apaga de novo quando
    // larga a conexão, e um cache sem credencial é apagado ao abrir.
    let _ = apagar_arquivos(caminho);
}

/// A estação do contrato como a janela a mostra.
pub fn estacao_da_janela(estacao: &Estacao, esta: bool) -> sbf_inventario::alteracao::Estacao {
    use sbf_inventario::alteracao::Credencial;
    use sbf_inventario::referencia::{Ref, Tipo};
    let sistema = match estacao.sistema.as_str() {
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "macOS",
        outro => outro,
    };
    sbf_inventario::alteracao::Estacao {
        alvo: Ref::nova(Tipo::Estacao, estacao.id.to_string()),
        nome: estacao.nome.clone(),
        sistema: sistema.to_owned(),
        ultima_sincronizacao: estacao.ultimo_contato.clone(),
        credencial: if estacao.revogada {
            Credencial::Revogada
        } else {
            Credencial::Ativa
        },
        esta,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn versoes_comparam_por_numero() {
        assert_eq!(comparar_versoes("0.10.0", "0.9.3"), Ordering::Greater);
        assert_eq!(comparar_versoes("1.0.0", "1.0.0"), Ordering::Equal);
        assert_eq!(comparar_versoes("v0.2.0", "0.3.0"), Ordering::Less);
    }
}
