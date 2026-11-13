//! O formato no fio é o contrato: estes testes o fixam (EST-18 e o corpo de Q6/Q7).

use sbf_contrato::*;
use serde_json::{Value, json};

fn id(n: u128) -> Id {
    Id::from_u128(n)
}

fn movimentacao() -> Movimentacao {
    Movimentacao {
        id: id(1),
        versao: Versao("1763049600000000".into()),
        conta: id(2),
        tipo: TipoDeMovimentacao::Saida,
        valor: 30,
        descricao: Some("Padaria Exemplo".into()),
        data: "2026-11-10".into(),
        competencia: None,
        fatura: None,
        parcela_de_divida: None,
        categoria: Some(id(3)),
        prioridade: Some(Prioridade::Essencial),
        confirmada: true,
        notas: None,
        origem: OrigemDaMovimentacao::Manual,
        ocorrencia: None,
        fornecedor: None,
        documento: None,
        tags: vec![id(4)],
        criada_em: "2026-11-10T12:00:00Z".into(),
    }
}

#[test]
fn caminho_da_colecao_e_o_mesmo_nome_do_fio() {
    for c in Colecao::TODAS {
        let fio = serde_json::to_value(c).unwrap();
        assert_eq!(fio, Value::String(c.caminho().into()));
        assert_eq!(Colecao::do_caminho(c.caminho()), Some(c));
    }
    assert_eq!(Colecao::do_caminho("transactions"), None);
    assert_eq!(Colecao::ItensDeFatura.caminho(), "itens-de-fatura");
}

#[test]
fn todas_as_colecoes_aparecem_uma_vez() {
    let mut vistas = std::collections::HashSet::new();
    for c in Colecao::TODAS {
        assert!(vistas.insert(c), "{c:?} repetida");
    }
    assert_eq!(vistas.len(), 19);
}

#[test]
fn linhas_da_colecao_levam_o_nome_e_dinheiro_em_centavos_inteiros() {
    let linhas = LinhasDaColecao::Movimentacoes(vec![movimentacao()]);
    let fio = serde_json::to_value(&linhas).unwrap();
    assert_eq!(fio["colecao"], "movimentacoes");
    assert_eq!(fio["linhas"][0]["valor"], json!(30));
    assert!(fio["linhas"][0]["valor"].is_i64());
    assert_eq!(fio["linhas"][0]["tipo"], "saida");
    assert_eq!(fio["linhas"][0]["versao"], "1763049600000000");
    let volta: LinhasDaColecao = serde_json::from_value(fio).unwrap();
    assert_eq!(volta, linhas);
    assert_eq!(volta.colecao(), Colecao::Movimentacoes);
    assert_eq!(volta.quantas(), 1);
}

#[test]
fn proposta_de_mudar_categoria_tem_o_formato_fixado() {
    let proposta = PropostaDeAlteracao {
        id: id(9),
        acao: Acao::MudarCategoriaDaMovimentacao {
            movimentacao: id(1),
            categoria: Some(id(5)),
        },
        item: ItemAlterado {
            colecao: Colecao::Movimentacoes,
            id: id(1),
        },
        versoes_de_base: vec![VersaoDeBase {
            colecao: Colecao::Movimentacoes,
            id: id(1),
            versao: Versao("1763049600000000".into()),
        }],
        feita_em: "2026-11-13T21:00:00Z".into(),
    };
    let esperado = json!({
        "id": id(9).to_string(),
        "acao": {
            "tipo": "mudar_categoria_da_movimentacao",
            "movimentacao": id(1).to_string(),
            "categoria": id(5).to_string()
        },
        "item": { "colecao": "movimentacoes", "id": id(1).to_string() },
        "versoes_de_base": [
            { "colecao": "movimentacoes", "id": id(1).to_string(), "versao": "1763049600000000" }
        ],
        "feita_em": "2026-11-13T21:00:00Z"
    });
    assert_eq!(serde_json::to_value(&proposta).unwrap(), esperado);
    let volta: PropostaDeAlteracao = serde_json::from_value(esperado).unwrap();
    assert_eq!(volta, proposta);
}

#[test]
fn tirar_a_categoria_e_categoria_nula() {
    let acao: Acao = serde_json::from_value(json!({
        "tipo": "mudar_categoria_da_movimentacao",
        "movimentacao": id(1).to_string(),
        "categoria": null
    }))
    .unwrap();
    assert_eq!(
        acao,
        Acao::MudarCategoriaDaMovimentacao {
            movimentacao: id(1),
            categoria: None
        }
    );
}

#[test]
fn acao_desconhecida_nao_desserializa() {
    let r: Result<Acao, _> = serde_json::from_value(json!({ "tipo": "apagar_tudo" }));
    assert!(r.is_err());
}

#[test]
fn aceita_e_recusada_lado_a_lado() {
    let proposta = PropostaDeAlteracao {
        id: id(9),
        acao: Acao::MudarCategoriaDaMovimentacao {
            movimentacao: id(1),
            categoria: None,
        },
        item: ItemAlterado {
            colecao: Colecao::Movimentacoes,
            id: id(1),
        },
        versoes_de_base: vec![],
        feita_em: "2026-11-13T21:00:00Z".into(),
    };
    let aceita = RespostaDaAlteracao::Aceita {
        id: id(9),
        linhas: vec![LinhaAgora::Movimentacoes(Box::new(movimentacao()))],
    };
    let fio = serde_json::to_value(&aceita).unwrap();
    assert_eq!(fio["resultado"], "aceita");
    assert_eq!(fio["linhas"][0]["colecao"], "movimentacoes");
    assert_eq!(fio["linhas"][0]["linha"]["valor"], json!(30));

    let recusada = RespostaDaAlteracao::Recusada {
        id: id(9),
        motivo: CodigoDeErro::VersaoDeBaseDivergente,
        mensagem: "a movimentação mudou no servidor".into(),
        sua_versao: Box::new(proposta),
        no_servidor_agora: vec![LinhaAgora::Movimentacoes(Box::new(movimentacao()))],
    };
    let fio = serde_json::to_value(&recusada).unwrap();
    assert_eq!(fio["resultado"], "recusada");
    assert_eq!(fio["motivo"], "versao_de_base_divergente");
    assert_eq!(
        fio["sua_versao"]["acao"]["tipo"],
        "mudar_categoria_da_movimentacao"
    );
    assert_eq!(fio["no_servidor_agora"][0]["colecao"], "movimentacoes");
    let volta: RespostaDaAlteracao = serde_json::from_value(fio).unwrap();
    assert_eq!(volta, recusada);
    assert_eq!(volta.id(), id(9));
}

#[test]
fn erro_tem_codigo_estavel() {
    let erro = ErroDaApi {
        codigo: CodigoDeErro::EstacaoRevogada,
        mensagem: "esta estação foi revogada".into(),
        contratos_aceitos: None,
    };
    let fio = serde_json::to_value(&erro).unwrap();
    assert_eq!(
        fio,
        json!({ "codigo": "estacao_revogada", "mensagem": "esta estação foi revogada" })
    );
    let antigo = ErroDaApi {
        codigo: CodigoDeErro::ContratoAntigo,
        mensagem: "atualize o app".into(),
        contratos_aceitos: Some(vec![1, 2]),
    };
    let fio = serde_json::to_value(&antigo).unwrap();
    assert_eq!(fio["codigo"], "contrato_antigo");
    assert_eq!(fio["contratos_aceitos"], json!([1, 2]));
}

#[test]
fn debug_nunca_mostra_credencial_nem_id_token() {
    let estacao = Estacao {
        id: id(7),
        nome: "Notebook".into(),
        sistema: "linux".into(),
        pareada_em: "2026-11-13T21:00:00Z".into(),
        ultimo_contato: None,
        revogada: false,
        revogada_em: None,
    };
    let p = Pareamento {
        estacao,
        credencial: "sbfe_SEGREDO-DE-TESTE".into(),
    };
    assert!(!format!("{p:?}").contains("SEGREDO"));
    let pedido = PedidoDePareamento {
        id_token: "eyJ.SEGREDO.x".into(),
        nonce: "NONCE-SEGREDO".into(),
        nome: "Notebook".into(),
        sistema: "linux".into(),
    };
    assert!(!format!("{pedido:?}").contains("SEGREDO"));
}

#[test]
fn estado_leva_os_documentos_a_revisar() {
    let estado = Estado {
        contrato: 1,
        contratos_aceitos: vec![1],
        regras: "0.1.0".into(),
        hora_do_servidor: "2026-11-13T21:00:00.000000Z".into(),
        dia_de_corte: 20,
        fuso: "America/Sao_Paulo".into(),
        documentos_a_revisar: 3,
        estacao: Estacao {
            id: id(7),
            nome: "Notebook".into(),
            sistema: "linux".into(),
            pareada_em: "2026-11-13T21:00:00Z".into(),
            ultimo_contato: None,
            revogada: false,
            revogada_em: None,
        },
    };
    let fio = serde_json::to_value(&estado).unwrap();
    assert_eq!(fio["documentos_a_revisar"], json!(3));
    assert_eq!(fio["dia_de_corte"], json!(20));
    let volta: Estado = serde_json::from_value(fio).unwrap();
    assert_eq!(volta, estado);
}

#[test]
fn prefixo_da_versao() {
    assert_eq!(prefixo(VERSAO_DO_CONTRATO), "/api/v1");
    const { assert!(PRIMEIRA_VERSAO_DO_CONTRATO <= VERSAO_DO_CONTRATO) };
}
