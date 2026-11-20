//! RITA-08: o painel Rita desenhado sem janela (`egui::Context::run_ui`), com provedor falso e
//! cofre fictício temporário: acompanha o item, mostra o cartão e a linha de tokens e custo, e o
//! cartão de um rascunho que derruba outro padrão pede confirmação para ativar.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sbf_ia::{ChamadaDeFerramenta, Consumo, FalsoDeConversa, RespostaDeConversa};
use sbf_janela::rita_ui::{AcaoDeRita, Ambiente, EstadoDoCartao, ItemDaTela, RitaUi};
use sbf_regras::Quem;
use sbf_regras::catalogo::{Catalogo, NovaVersao};
use sbf_regras::definicao::Definicao;
use sbf_regras::{Autor, Estado};
use sbf_rita::{DadosDaTela, GuardaEmMemoria, ProvedorDaRita, entrada_de_documento};
use serde_json::json;

const TEXTO: &str = "Comprovante Pix\nBanco Exemplo\nValor: R$ 150,00\nData: 10/11/2026\n";

fn entrada() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(480.0, 720.0),
        )),
        ..Default::default()
    }
}

fn desenhar(ctx: &egui::Context, rita: &mut RitaUi, item: Option<&ItemDaTela>) -> Vec<AcaoDeRita> {
    let mut acoes = Vec::new();
    let mut saida = ctx.run_ui(entrada(), |ui| {
        acoes = rita.desenhar(ui, item);
    });
    saida.textures_delta.clear();
    acoes
}

fn esperar(r: &mut RitaUi) {
    let t0 = Instant::now();
    while r.trabalhando() {
        r.sondar();
        assert!(t0.elapsed() < Duration::from_secs(20), "demorou");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[derive(Debug)]
struct SemScript;

impl sbf_regras::script::ExecutorDeScript for SemScript {
    fn executar(
        &self,
        _: &sbf_regras::script::PedidoDeScript,
    ) -> Result<sbf_regras::script::RespostaDoScript, sbf_regras::script::ErroDoScript> {
        Err(sbf_regras::script::ErroDoScript::Processo(
            "sem scripts".into(),
        ))
    }

    fn verificar(&self, _: &str) -> Result<(), sbf_regras::script::ErroDoScript> {
        Ok(())
    }
}

fn definicao(contem: &[&str]) -> serde_json::Value {
    json!({
        "tipo_documental": "pix-banco-exemplo",
        "detectar": {"todos": contem.iter().map(|c| json!({"contem": c})).collect::<Vec<_>>()},
        "extrair": {"valor_centavos": {"regex": "Valor:\\s*R\\$\\s*([\\d.,]+)", "formato": "centavos"}},
        "obrigatorios": ["valor_centavos"]
    })
}

fn montar(cofre: &std::path::Path) -> (RitaUi, ItemDaTela) {
    {
        // Um padrão ativo mais geral: o rascunho de Rita, mais específico, vai tomá-lo.
        let mut c = Catalogo::abrir(&cofre.join("regras.sqlite")).unwrap();
        let def: Definicao = serde_json::from_value(json!({
            "tipo_documental": "pix-geral",
            "detectar": {"todos": [{"contem": "comprovante pix"}]},
            "extrair": {"valor_centavos": {"regex": "Valor:\\s*R\\$\\s*([\\d.,]+)", "formato": "centavos"}}
        }))
        .unwrap();
        c.salvar_rascunho(NovaVersao {
            regra: "pix-geral".into(),
            nome: "Pix geral".into(),
            definicao: def,
            script: None,
            autor: Autor::Usuario,
            motivo: "teste".into(),
        })
        .unwrap();
        c.ativar("pix-geral", 1, &Quem::Usuario, None).unwrap();
        assert_eq!(c.vigentes().unwrap()[0].estado, Estado::Ativa);
    }
    sbf_rita::registro::inventario_minimo_para_testes(cofre);
    let mut r = RitaUi::default();
    r.ligar(Ambiente {
        cofre: cofre.to_path_buf(),
        guarda: Arc::new(GuardaEmMemoria::default()),
        executor: Arc::new(SemScript),
        programa: PathBuf::from("/usr/bin/sbf"),
    });
    let e = entrada_de_documento(
        &"ab".repeat(32),
        Some("c.txt"),
        Some("text/plain"),
        Some(TEXTO),
    );
    let item = ItemDaTela {
        chave: "ab".repeat(32),
        rotulo: "c.txt".into(),
        tela: DadosDaTela::de_documento(cofre, e),
    };
    (r, item)
}

#[test]
fn rita08_acompanha_a_selecao_destaca_e_mostra_tokens_custo_e_confirmacao() {
    let d = tempfile::tempdir().unwrap();
    let (mut rita, item) = montar(d.path());
    let ctx = egui::Context::default();
    sbf_janela::tema::instalar_fontes(&ctx);

    // Sem item: diz o que pode fazer. Com item sem conversa: o campo de pedido.
    assert!(desenhar(&ctx, &mut rita, None).is_empty());
    assert!(desenhar(&ctx, &mut rita, Some(&item)).is_empty());

    // Um pedido: o provedor falso cria o rascunho (que derruba o `pix-geral`).
    let falso = Arc::new(FalsoDeConversa::com_roteiro(vec![Ok(RespostaDeConversa {
        custo_ausente: false,
        texto: None,
        chamadas: vec![ChamadaDeFerramenta {
            id: "1".into(),
            nome: "propor_padrao".into(),
            argumentos: json!({
                "regra": "pix-banco-exemplo", "nome": "Pix Banco Exemplo",
                "definicao": definicao(&["comprovante pix", "banco exemplo"]),
            }),
        }],
        consumo: Consumo {
            tokens_entrada: 1500,
            tokens_saida: 80,
            custo_dezmilesimos_usd: 0,
            nominal: false,
        },
        modelo: "falso-1".into(),
    })]));
    rita.iniciar_com(&item, "cria o padrão", ProvedorDaRita::Conversa(falso));
    // Enquanto trabalha, o painel desenha (spinner e Parar) sem pânico.
    desenhar(&ctx, &mut rita, Some(&item));
    esperar(&mut rita);
    desenhar(&ctx, &mut rita, Some(&item));

    let k = rita.cartao(&item.chave).expect("cartão");
    assert!(k.pre_teste.passou);
    assert_eq!(k.pre_teste.derrubadas[0].regra, "pix-geral");
    assert!(
        k.pre_teste.pede_confirmacao(),
        "derrubar outro padrão pede confirmação"
    );
    assert_eq!(k.estado, EstadoDoCartao::Aberto);
    assert_eq!(
        rita.linha_de_consumo(&item.chave).as_deref(),
        Some("Última resposta: 1.500 tokens de entrada · 80 de saída · local, sem custo")
    );

    // A seleção mudou para outro item: o painel mostra a conversa dele (vazia), a primeira fica.
    let mut outro = item.clone();
    outro.chave = "cd".repeat(32);
    outro.rotulo = "outro.txt".into();
    assert!(desenhar(&ctx, &mut rita, Some(&outro)).is_empty());
    assert!(rita.cartao(&outro.chave).is_none());
    assert!(rita.cartao(&item.chave).is_some());

    // Destacar e voltar.
    assert!(!rita.flutuante);
    rita.flutuante = true;
    desenhar(&ctx, &mut rita, Some(&item));
    rita.flutuante = false;

    // O rascunho não foi ativado por ninguém.
    let c = Catalogo::abrir(&d.path().join("regras.sqlite")).unwrap();
    let vigentes: Vec<_> = c.vigentes().unwrap().into_iter().map(|v| v.regra).collect();
    assert_eq!(vigentes, vec!["pix-geral".to_owned()]);
    // A janela ativa quando o usuário confirma: o painel só registra o resultado.
    rita.ativacao_feita("pix-banco-exemplo", 1, Ok(()));
    assert_eq!(
        rita.cartao(&item.chave).map(|k| k.estado),
        Some(EstadoDoCartao::Ativado)
    );
}
