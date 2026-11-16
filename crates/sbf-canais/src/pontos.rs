//! Os pontos de atenção da estação como fonte de alertas (spec 10): implementa
//! [`FonteDeAlertas`] sobre `pontos.sqlite`.
//!
//! **Desligada por padrão.** Só entra quando `SBF_PONTOS_BANCO` aponta para o arquivo, o que
//! o usuário faz quando o canal está vinculado e ele quer receber os pontos no Telegram. A
//! VPS não tem `pontos.sqlite` (a análise roda na estação): sem o arquivo, a fonte não
//! devolve nada. O texto passa pelo nível de confiança do destinatário, como os outros.
//!
//! Cada ponto **aberto** vira um alerta; a chave é `ponto:<regra>:<chave>` e o estado é o
//! instante em que o ponto mudou pela última vez: um ponto resolvido some, e um que reabre
//! por mudança da evidência avisa de novo (o envio só repete quando o estado muda).

use std::path::PathBuf;

use sbf_coletor::config::opcional;
use sbf_livro::{Data, LivroCaixa};
use sbf_pontos::{Armazem, Estado, Filtro};

use crate::alertas::{Alerta, Dados, FonteDeAlertas, TipoDeAlerta};

/// A variável que liga a fonte.
pub const VARIAVEL: &str = "SBF_PONTOS_BANCO";

#[derive(Debug, Clone)]
pub struct FonteDePontos {
    caminho: PathBuf,
}

impl FonteDePontos {
    #[must_use]
    pub fn nova(caminho: impl Into<PathBuf>) -> Self {
        Self {
            caminho: caminho.into(),
        }
    }
}

impl FonteDeAlertas for FonteDePontos {
    fn alertas(&self, _livro: &LivroCaixa, _hoje: Data) -> Vec<Alerta> {
        if !self.caminho.is_file() {
            return Vec::new();
        }
        let Ok(armazem) = Armazem::abrir(&self.caminho) else {
            return Vec::new();
        };
        let Ok(pontos) = armazem.listar(Filtro {
            area: None,
            estado: Some(Estado::Aberto),
        }) else {
            return Vec::new();
        };
        pontos
            .into_iter()
            .map(|p| Alerta {
                tipo: TipoDeAlerta::PontoDeAtencao,
                chave: format!("ponto:{}:{}", p.regra, p.chave),
                estado: p.atualizado_em.clone(),
                dados: Dados::Ponto {
                    titulo: p.titulo,
                    area: p.area.rotulo().to_owned(),
                    impacto: p.impacto.map(|i| i.texto()),
                },
            })
            .collect()
    }
}

/// As fontes extras pedidas pelo ambiente: vazio, a menos que `SBF_PONTOS_BANCO` esteja posto.
#[must_use]
pub fn do_ambiente() -> Vec<Box<dyn FonteDeAlertas>> {
    opcional(VARIAVEL)
        .map(|c| vec![Box::new(FonteDePontos::nova(c)) as Box<dyn FonteDeAlertas>])
        .unwrap_or_default()
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::alertas::{Limites, avaliar};
    use crate::confianca::Nivel;
    use sbf_pontos::{Area, Candidato, Evidencia, Impacto, Unidade};

    fn candidato(chave: &str, titulo: &str) -> Candidato {
        Candidato {
            regra: "assinatura-reajuste",
            chave: chave.into(),
            area: Area::Dinheiro,
            titulo: titulo.into(),
            detalhe: String::new(),
            evidencia: vec![Evidencia::nova("movimentacao", "m1", "x")],
            impacto: Some(Impacto {
                valor: 18_000,
                unidade: Unidade::CentavosBrl,
                periodo: "ano".into(),
                estimativa: true,
            }),
            assinatura: chave.into(),
        }
    }

    fn hoje() -> Data {
        Data::nova(2026, 11, 14).expect("data")
    }

    #[test]
    fn desligada_por_padrao() {
        // Sem a variável (e sem arquivo), nada entra na avaliação.
        assert!(
            std::env::var(VARIAVEL).is_err(),
            "o ambiente do teste não define {VARIAVEL}"
        );
        assert!(do_ambiente().is_empty());
        let f = FonteDePontos::nova("/nao/existe/pontos.sqlite");
        assert!(f.alertas(&LivroCaixa::default(), hoje()).is_empty());
    }

    #[test]
    fn pontos_abertos_viram_alertas_com_estimativa_marcada() {
        let pasta = tempfile::tempdir().expect("pasta");
        let caminho = pasta.path().join("pontos.sqlite");
        let mut z = Armazem::abrir(&caminho).expect("armazém");
        z.aplicar(
            &[
                candidato("reajuste:f1", "Filmes Exemplo subiu de preço"),
                candidato("reajuste:f2", "Seguro Exemplo subiu de preço"),
            ],
            &["assinatura-reajuste"],
            "2026-11-14T12:00:00Z",
        )
        .expect("aplicar");
        let ignorado = z.listar(Filtro::default()).expect("l")[0].id;
        z.ignorar(ignorado, "já sei", "2026-11-14T12:00:00Z")
            .expect("ignorar");

        let fonte = FonteDePontos::nova(&caminho);
        let r = avaliar(
            &LivroCaixa::default(),
            hoje(),
            &Limites::default(),
            &[Box::new(fonte)],
        );
        assert_eq!(r.len(), 1, "só o aberto");
        assert_eq!(r[0].tipo, TipoDeAlerta::PontoDeAtencao);
        let linha = crate::texto::linha(&r[0], Nivel::Mascarado);
        assert!(
            linha.contains("subiu de preço") && linha.contains("[Dinheiro]"),
            "{linha}"
        );
        assert!(linha.contains("estimativa: R$ 180,00 por ano"), "{linha}");
    }
}
