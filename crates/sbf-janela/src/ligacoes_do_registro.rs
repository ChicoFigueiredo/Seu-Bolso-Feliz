//! As ligações da página de um registro da Pluggy com o resto do app: as ocorrências do
//! planejamento a que o movimento está ligado ("Concilia") e a regra de reconhecimento
//! vigente que o reconhece ("Reconhecido pela regra"). O que o inventário não sabe: ele não
//! conhece o planejamento nem o catálogo de regras, então a janela junta as três fontes.
//!
//! Só leitura. O que decide (`montar`) não toca egui; `bloco` só desenha.

use std::path::Path;

use egui::{RichText, Ui};
use rusqlite::{Connection, OpenFlags};
use sbf_inventario::{Ref, Tipo};
use sbf_planejamento::{Centavos, IdOcorrencia, MovimentoDoExtrato, Planejamento};

use crate::componentes;
use crate::controle::{competencia_texto, reais};
use crate::navegacao::{Destino, Pedido};
use crate::regras::FonteDasRegras;
use crate::tema;

/// Uma ocorrência do planejamento ligada ao movimento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcorrenciaLigada {
    pub id: IdOcorrencia,
    /// O nome do compromisso (ou o id dele, se o planejamento não o conhece mais).
    pub nome: String,
    pub valor: Centavos,
    /// `false`: ainda é uma sugestão à espera da confirmação.
    pub confirmada: bool,
}

/// A regra de reconhecimento vigente que reconhece o registro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegraQueReconhece {
    pub id: String,
    pub nome: String,
    pub versao: u32,
}

/// Tudo o que a página mostra além do que o inventário sabe.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LigacoesDoRegistro {
    pub concilia: Vec<OcorrenciaLigada>,
    pub regra: Option<RegraQueReconhece>,
}

/// O que um clique numa ligação pede; aplicado em [`crate::Janela::processar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PedidoDeLigacao {
    /// Abre a página do registro do inventário de onde veio o movimento (`pluggy:txn_…`).
    Registro { movimento: String, flutuante: bool },
    /// Mostra a ocorrência no Controle mensal.
    Ocorrencia {
        ocorrencia: IdOcorrencia,
        flutuante: bool,
    },
    /// Abre a página da regra na área Regras de reconhecimento.
    Regra { regra: String, flutuante: bool },
}

/// Junta as ligações do movimento. Cada fonte ausente ou com erro simplesmente não
/// contribui: a página nunca falha por causa delas.
pub fn montar(
    plan: Option<&dyn Planejamento>,
    regras: Option<&FonteDasRegras>,
    movimento: &MovimentoDoExtrato,
) -> LigacoesDoRegistro {
    let concilia = plan.map(|p| ocorrencias_ligadas(p, &movimento.movimento));
    LigacoesDoRegistro {
        concilia: concilia.unwrap_or_default(),
        regra: regras.and_then(|r| regra_que_reconhece(r, movimento)),
    }
}

fn ocorrencias_ligadas(plan: &dyn Planejamento, movimento: &str) -> Vec<OcorrenciaLigada> {
    let mut v: Vec<OcorrenciaLigada> = plan
        .conciliacoes_do_movimento(movimento)
        .unwrap_or_default()
        .into_iter()
        .map(|c| {
            let nome = plan
                .recorrente(&c.ocorrencia.recorrente)
                .ok()
                .flatten()
                .map_or_else(|| c.ocorrencia.recorrente.clone(), |r| r.nome);
            OcorrenciaLigada {
                id: c.ocorrencia,
                nome,
                valor: c.valor,
                confirmada: c.confirmada,
            }
        })
        .collect();
    v.sort_by(|a, b| {
        (b.confirmada, b.id.competencia, &b.nome).cmp(&(a.confirmada, a.id.competencia, &a.nome))
    });
    v
}

fn regra_que_reconhece(
    fonte: &FonteDasRegras,
    movimento: &MovimentoDoExtrato,
) -> Option<RegraQueReconhece> {
    let catalogo = fonte.catalogo.lock().ok()?;
    let motor = sbf_regras::motor::Motor::das_vigentes(&catalogo, fonte.executor.clone()).ok()?;
    let rec = sbf_reconhecimento::regras::ler_registro(
        &motor,
        &sbf_reconhecimento::regras::entrada_do_registro(movimento),
    )?;
    let nome = catalogo
        .regras()
        .ok()
        .and_then(|rs| rs.into_iter().find(|r| r.id == rec.regra))
        .map_or_else(|| rec.regra.clone(), |r| r.nome);
    Some(RegraQueReconhece {
        id: rec.regra,
        nome,
        versao: rec.versao,
    })
}

/// O registro do inventário com este `external_ref`, por uma conexão só de leitura.
///
/// # Errors
/// O texto do erro ao abrir o banco ou consultar.
pub fn registro_do_movimento(banco: &Path, movimento: &str) -> Result<Option<Ref>, String> {
    let conn = Connection::open_with_flags(
        banco,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    sbf_inventario::relacoes::registro_pelo_id_externo(&conn, movimento).map_err(|e| e.to_string())
}

/// O movimento do extrato de um registro (para as ligações): o `external_ref` está no campo
/// "Id externo" da página.
pub fn movimento_do_registro(
    banco: &Path,
    externo: &str,
) -> Result<Option<MovimentoDoExtrato>, String> {
    sbf_planilha::movimentos::MovimentosDaPluggy::novo(banco)
        .do_registro(externo)
        .map_err(|e| e.to_string())
}

/// O `external_ref` que a página do registro mostra no campo "Id externo".
pub fn id_externo_do_item(item: &sbf_inventario::consulta::Item) -> Option<String> {
    item.campos.iter().find_map(|c| match &c.valor {
        sbf_inventario::consulta::Celula::Texto(t) if c.rotulo == "Id externo" => Some(t.clone()),
        _ => None,
    })
}

/// Só registros do extrato têm o que mostrar aqui.
pub fn se_aplica(alvo: &Ref) -> bool {
    alvo.tipo == Tipo::Registro
}

/// O bloco "Concilia" e "Reconhecido pela regra".
pub fn bloco(ui: &mut Ui, l: &LigacoesDoRegistro, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    ui.vertical(|ui| {
        componentes::rotulo_de_secao(ui, "Concilia");
        if l.concilia.is_empty() {
            ui.label(
                RichText::new("Nenhuma ocorrência do planejamento está ligada a este movimento.")
                    .color(p.tinta_suave),
            );
        }
        for o in &l.concilia {
            ui.horizontal_wrapped(|ui| {
                let texto = format!("{} · {}", o.nome, competencia_texto(o.id.competencia));
                let (_, clique) = componentes::link_cru(ui, &texto);
                if let Some(flutuante) = clique {
                    pedidos.push(Pedido::Ligacao(PedidoDeLigacao::Ocorrencia {
                        ocorrencia: o.id.clone(),
                        flutuante,
                    }));
                }
                ui.label(RichText::new(reais(o.valor)).color(p.tinta_suave));
                if !o.confirmada {
                    componentes::selo(ui, "sugestão", p.superficie_3, p.info);
                }
            });
        }
        ui.add_space(4.0);
        componentes::rotulo_de_secao(ui, "Reconhecido pelo padrão");
        match &l.regra {
            Some(r) => {
                ui.horizontal_wrapped(|ui| {
                    let (_, clique) =
                        componentes::link_cru(ui, &format!("{} · versão {}", r.nome, r.versao));
                    if let Some(flutuante) = clique {
                        pedidos.push(Pedido::Ligacao(PedidoDeLigacao::Regra {
                            regra: r.id.clone(),
                            flutuante,
                        }));
                    }
                });
            }
            None => {
                ui.label(
                    RichText::new("Nenhum padrão vigente reconhece este registro.")
                        .color(p.tinta_suave),
                );
            }
        }
    });
}

/// Destino do link "Controle mensal" (a área).
pub fn destino_do_controle() -> Destino {
    Destino::AreaFutura("controle")
}

/// Destino da área Regras de reconhecimento.
pub fn destino_das_regras() -> Destino {
    Destino::AreaFutura(crate::regras_ui::ROTA)
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::sync::{Arc, Mutex};

    use sbf_planejamento::Data;
    use sbf_regras::catalogo::{Catalogo, NovaVersao};
    use sbf_regras::definicao::{Condicao, Definicao, Extracao, FonteDaExtracao, Formato};
    use sbf_regras::{Autor, Quem, TipoDeEntrada};

    use crate::planejamento_ficticio::PlanejamentoFicticio;

    const DOC: &str = "000.000.000-00";

    fn movimento(id: &str, doc: Option<&str>) -> MovimentoDoExtrato {
        MovimentoDoExtrato {
            movimento: id.into(),
            data: Data::nova(2026, 11, 5).expect("data"),
            valor: Centavos(-15_000),
            conta_do_extrato: "CAIXA · 0001-1".into(),
            descricao: "PIX ENVIADO".into(),
            tipo: "debito".into(),
            contraparte: Some("Fulana de Tal".into()),
            contraparte_doc: doc.map(Into::into),
        }
    }

    fn fonte_com_a_regra_da_diarista() -> FonteDasRegras {
        let mut catalogo = Catalogo::em_memoria().expect("catálogo");
        let mut d = Definicao::simples("pix-da-diarista", "x");
        d.aplica_a = vec![TipoDeEntrada::Registro];
        d.detectar.todos = vec![Condicao::ContraparteDoc(vec![DOC.into()])];
        d.extrair.insert(
            "favorecido".into(),
            Extracao {
                regex: "(.+)".into(),
                grupo: 1,
                formato: Formato::Texto,
                fonte: FonteDaExtracao::Contraparte,
            },
        );
        let v = catalogo
            .salvar_rascunho(NovaVersao {
                regra: "pix-da-diarista".into(),
                nome: "Pix da diarista".into(),
                definicao: d,
                script: None,
                autor: Autor::Usuario,
                motivo: "teste".into(),
            })
            .expect("rascunho");
        catalogo
            .ativar(&v.regra, v.versao, &Quem::Usuario, None)
            .expect("ativar");
        FonteDasRegras::nova(Arc::new(Mutex::new(catalogo)), None)
    }

    #[test]
    fn a_regra_vigente_que_reconhece_o_registro_aparece_com_nome_e_versao() {
        let fonte = fonte_com_a_regra_da_diarista();
        let l = montar(None, Some(&fonte), &movimento("pluggy:txn_a", Some(DOC)));
        assert_eq!(
            l.regra,
            Some(RegraQueReconhece {
                id: "pix-da-diarista".into(),
                nome: "Pix da diarista".into(),
                versao: 1,
            })
        );
        assert!(l.concilia.is_empty());
    }

    #[test]
    fn registro_que_nenhuma_regra_reconhece_fica_sem_regra() {
        let fonte = fonte_com_a_regra_da_diarista();
        let l = montar(
            None,
            Some(&fonte),
            &movimento("pluggy:txn_b", Some("111.111.111-11")),
        );
        assert_eq!(l.regra, None);
    }

    #[test]
    fn concilia_lista_as_ocorrencias_do_planejamento_ligadas_ao_movimento() {
        let hoje = Data::nova(2026, 11, 18).expect("hoje");
        let plan = PlanejamentoFicticio::novo(hoje);
        let l = montar(
            Some(&plan),
            None,
            &movimento("pluggy:txn_ficticia_sal_202609", None),
        );
        assert_eq!(l.concilia.len(), 1);
        assert_eq!(l.concilia[0].nome, "Salário líquido (Exemplo)");
        assert_eq!(l.concilia[0].id.competencia.mes, 9);
        assert!(l.concilia[0].confirmada);
        assert!(
            montar(Some(&plan), None, &movimento("pluggy:txn_nada", None))
                .concilia
                .is_empty()
        );
    }

    #[test]
    fn acha_o_registro_do_movimento_pelo_id_externo_do_planejamento() {
        let dir = tempfile::tempdir().expect("pasta");
        let banco = dir.path().join("inventario.sqlite");
        let c = Connection::open(&banco).expect("banco");
        c.execute_batch(
            "CREATE TABLE registro (id INTEGER PRIMARY KEY, external_ref TEXT);
             INSERT INTO registro (id, external_ref) VALUES (41, 'pluggy:txn_a');",
        )
        .expect("esquema");
        drop(c);
        assert_eq!(
            registro_do_movimento(&banco, "pluggy:txn_a").expect("busca"),
            Some(Ref::nova(Tipo::Registro, "41"))
        );
        assert_eq!(
            registro_do_movimento(&banco, "pluggy:txn_b").expect("busca"),
            None
        );
        assert!(registro_do_movimento(&dir.path().join("nao-existe.sqlite"), "x").is_err());
    }
}
