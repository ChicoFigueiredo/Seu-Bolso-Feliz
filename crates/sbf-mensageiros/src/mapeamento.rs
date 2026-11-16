//! Da mensagem para o que o inventário guarda. Usa as **mesmas heurísticas** de áreas e tipos
//! do inventário (`sbf_coleta::areas`): o nome do anexo é classificado como numa pasta de área
//! pessoal (documento e texto copiam; foto também, pois a conversa foi escolhida a dedo; áudio,
//! vídeo e figurinha ficam de fora), e o texto vira achado quando parece financeiro.
//!
//! Origens: `whatsapp:<conversa>/<mensagem>` e `telegram:<conversa>/<mensagem>`. Quando a
//! mesma mensagem tem anexo **e** texto financeiro, o texto leva o sufixo `#texto`.

use std::sync::LazyLock;

use regex::Regex;
use sbf_coleta::areas::{
    Camada, Classe, Regra, Tratamento, extensao_de, nome_financeiro, tratamento_do_arquivo,
};

use crate::modelo::{Mensagem, Plataforma};

/// O que uma mensagem rende ao inventário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// O anexo da mensagem.
    Anexo {
        /// Chave da origem.
        origem: String,
        /// Nome do arquivo, sem pasta.
        nome: String,
        /// Extensão em minúsculas, sem ponto.
        extensao: String,
    },
    /// O texto da mensagem, que parece financeiro.
    Texto {
        /// Chave da origem.
        origem: String,
        /// O texto, para virar `.txt`.
        conteudo: String,
    },
}

/// `<plataforma>:<conversa>/<mensagem>`.
#[must_use]
pub fn chave_da_origem(p: Plataforma, m: &Mensagem) -> String {
    format!("{}:{}/{}", p.nome(), m.conversa, m.id)
}

/// A referência de conversa e mensagem (`<conversa>/<mensagem>`), para `mensagem_ref`.
#[must_use]
pub fn referencia(m: &Mensagem) -> String {
    format!("{}/{}", m.conversa, m.id)
}

static LINHA_DIGITAVEL: LazyLock<Regex> = LazyLock::new(|| {
    // Boleto bancário: 47 dígitos (cobrança) ou 48 (arrecadação), com pontos e espaços.
    Regex::new(r"(?:\d[ .\-]?){46,47}\d").expect("regex fixa")
});

/// O texto parece financeiro: palavra de comprovante, fatura, boleto, Pix e afins (a mesma
/// lista dos nomes de arquivo do inventário), uma linha digitável de boleto ou um Pix copia
/// e cola.
#[must_use]
pub fn texto_parece_financeiro(texto: &str) -> bool {
    let t = texto.trim();
    if t.len() < 4 {
        return false;
    }
    nome_financeiro(t) || t.contains("br.gov.bcb.pix") || LINHA_DIGITAVEL.is_match(t)
}

fn classe_pessoal() -> Classe {
    Classe {
        regra: Regra {
            camada: Camada::A,
            motivo: None,
            ignorar: false,
        },
        raiz: "mensageiros".to_owned(),
    }
}

/// Só o nome do arquivo: sem pastas, sem controle, sem `..`.
#[must_use]
pub fn nome_seguro(nome: &str) -> String {
    let base = nome.rsplit(['/', '\\']).next().unwrap_or("");
    let limpo: String = base.chars().filter(|c| !c.is_control()).collect();
    let limpo = limpo.trim().trim_start_matches('.').to_owned();
    if limpo.is_empty() {
        "anexo".to_owned()
    } else {
        limpo
    }
}

/// Os itens de uma mensagem (zero, um ou dois).
#[must_use]
pub fn itens_da(p: Plataforma, m: &Mensagem) -> Vec<Item> {
    let base = chave_da_origem(p, m);
    let mut itens = Vec::new();
    if let Some(a) = &m.anexo {
        let nome = nome_seguro(&a.nome);
        let classe = classe_pessoal();
        if matches!(
            tratamento_do_arquivo(&nome, &classe),
            Tratamento::Copiar | Tratamento::Hash
        ) {
            itens.push(Item::Anexo {
                origem: base.clone(),
                extensao: extensao_de(&nome),
                nome,
            });
        }
    }
    if let Some(t) = &m.texto
        && texto_parece_financeiro(t)
    {
        let origem = if m.anexo.is_some() {
            format!("{base}#texto")
        } else {
            base
        };
        itens.push(Item::Texto {
            origem,
            conteudo: t.trim().to_owned(),
        });
    }
    itens
}

#[cfg(test)]
#[allow(
    clippy::field_reassign_with_default,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod testes {
    use super::*;
    use crate::modelo::Anexo;

    fn msg(texto: Option<&str>, anexo: Option<&str>) -> Mensagem {
        Mensagem {
            id: "m1".into(),
            conversa: "c9".into(),
            quando: 1_700_000_000,
            texto: texto.map(str::to_owned),
            anexo: anexo.map(|n| Anexo {
                nome: n.into(),
                mime: None,
                tamanho: None,
            }),
        }
    }

    #[test]
    fn texto_financeiro_e_conversa_comum() {
        assert!(texto_parece_financeiro("segue o comprovante do pix"));
        assert!(texto_parece_financeiro("Boleto da Luz vence dia 10"));
        assert!(texto_parece_financeiro(
            "23793.38128 60000.000003 12345.678901 1 98760000012345"
        ));
        assert!(texto_parece_financeiro(
            "00020126580014br.gov.bcb.pix0136abc"
        ));
        assert!(!texto_parece_financeiro("bom dia, vamos almoçar?"));
        assert!(!texto_parece_financeiro("ok"));
    }

    #[test]
    fn documento_e_foto_viram_achado_e_audio_nao() {
        let p = Plataforma::Whatsapp;
        assert_eq!(
            itens_da(p, &msg(None, Some("boleto.pdf"))),
            vec![Item::Anexo {
                origem: "whatsapp:c9/m1".into(),
                nome: "boleto.pdf".into(),
                extensao: "pdf".into()
            }]
        );
        assert_eq!(itens_da(p, &msg(None, Some("IMG-0001.jpg"))).len(), 1);
        assert!(itens_da(p, &msg(None, Some("PTT-0001.opus"))).is_empty());
        assert!(itens_da(p, &msg(None, Some("VID-0001.mp4"))).is_empty());
        assert!(itens_da(p, &msg(Some("oi"), None)).is_empty());
    }

    #[test]
    fn anexo_mais_texto_financeiro_dao_duas_origens() {
        let it = itens_da(
            Plataforma::Telegram,
            &msg(Some("comprovante do aluguel"), Some("foto.png")),
        );
        let origens: Vec<&str> = it
            .iter()
            .map(|i| match i {
                Item::Anexo { origem, .. } | Item::Texto { origem, .. } => origem.as_str(),
            })
            .collect();
        assert_eq!(origens, ["telegram:c9/m1", "telegram:c9/m1#texto"]);
        let so_texto = itens_da(Plataforma::Telegram, &msg(Some("fatura do cartão"), None));
        assert!(matches!(&so_texto[0], Item::Texto { origem, .. } if origem == "telegram:c9/m1"));
    }

    #[test]
    fn nome_do_anexo_nao_escapa_da_pasta() {
        assert_eq!(nome_seguro("../../etc/passwd"), "passwd");
        assert_eq!(nome_seguro("a\\b\\c.pdf"), "c.pdf");
        assert_eq!(nome_seguro(".bashrc"), "bashrc");
        assert_eq!(nome_seguro(""), "anexo");
    }
}
