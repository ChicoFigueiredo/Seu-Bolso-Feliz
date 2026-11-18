//! As ferramentas de regras de reconhecimento contra um `regras.sqlite` temporário, com regras
//! fictícias (nunca o cofre real). O inventário é o banco fictício da triagem.
#![allow(clippy::unwrap_used)]

#[path = "../../sbf-triagem/tests/ajuda/mod.rs"]
#[allow(unused_imports)]
mod ajuda;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ajuda::{Banco, banco_limpo};
use sbf_mcp::Nucleo;
use sbf_mcp::regras::{
    FERRAMENTAS, PedidoDeProposta, PedidoDeRegra, PedidoDeSimulacao, PedidoDeTeste, Regras,
};
use sbf_mcp::servidor::Servidor;
use sbf_regras::catalogo::{Catalogo, Esperado, NovaVersao, VersaoDeRegra};
use sbf_regras::definicao::Definicao;
use sbf_regras::script::ExecutorDeScript;
use sbf_regras::{
    Autor, Canal, Dicas, Entrada, Origem, Quem, TipoDeEntrada, VERSAO_DO_ESQUEMA, Valor,
};
use sbf_script::ExecutorNoProcesso;
use serde_json::{Value, json};
use tempfile::TempDir;

const PIX_ID: &str = "comprovante-pix-banco-exemplo";
const PIX_TIPO: &str = "comprovante-pix-banco-exemplo";
const PIX_NOME: &str = "Comprovante Pix · Banco Exemplo";
const TEXTO_PIX: &str =
    "Comprovante Pix\nBanco Exemplo\nValor: R$ 150,00\nData: 10/11/2026\nAutenticação: ABC123XYZ";
const TEXTO_PIX_SEM_AUTENTICACAO: &str =
    "Comprovante Pix\nBanco Exemplo\nValor: R$ 99,90\nData: 12/11/2026";

struct Cenario {
    _banco: Banco,
    _dir: TempDir,
    caminho: PathBuf,
    nucleo: Nucleo,
}

// ------------------------------------------------------------------------ o cenário

/// As definições e os textos fictícios do comprovante de Pix.
fn def_pix_json() -> Value {
    json!({
        "versao_do_esquema": 1,
        "tipo_documental": PIX_TIPO,
        "aplica_a": ["documento"],
        "prioridade": 0,
        "detectar": { "todos": [{ "contem": "comprovante pix" }, { "contem": "banco exemplo" }] },
        "extrair": {
            "valor_centavos": { "regex": r"Valor:\s*R\$\s*([\d.,]+)", "formato": "centavos" },
            "data": { "regex": r"Data:\s*(\d{2}/\d{2}/\d{4})", "formato": "data" },
            "autenticacao": { "regex": r"Autentica\S+:\s*(\S+)", "formato": "texto" }
        },
        "obrigatorios": ["valor_centavos"],
        "propor": [{ "provar_registro": { "janela_dias": 2 } }]
    })
}

fn def_pix() -> Definicao {
    serde_json::from_value(def_pix_json()).unwrap()
}

/// A mesma regra, mas que exige a autenticação: uma versão nova e diferente.
fn def_pix_exige_autenticacao() -> Value {
    let mut v = def_pix_json();
    v["obrigatorios"] = json!(["valor_centavos", "autenticacao"]);
    v
}

fn entrada(id: &str, texto: &str) -> Entrada {
    Entrada {
        versao_do_esquema: VERSAO_DO_ESQUEMA,
        id: id.to_owned(),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal: Canal::Manual,
            id_externo: None,
        },
        sha256: None,
        mime: Some("text/plain".to_owned()),
        nome: Some("comprovante.txt".to_owned()),
        texto: Some(texto.to_owned()),
        dicas: Dicas::default(),
        momento: None,
    }
}

fn nova(regra: &str, nome: &str, definicao: Definicao, script: Option<&str>) -> NovaVersao {
    NovaVersao {
        regra: regra.to_owned(),
        nome: nome.to_owned(),
        definicao,
        script: script.map(str::to_owned),
        autor: Autor::Usuario,
        motivo: "teste".to_owned(),
    }
}

/// O catálogo com a regra do Pix: versão 1 ativa, com um exemplo que passa.
fn semear(caminho: &Path) {
    let mut c = Catalogo::abrir(caminho).unwrap();
    let v = c
        .salvar_rascunho(nova(PIX_ID, PIX_NOME, def_pix(), None))
        .unwrap();
    c.salvar_exemplo(
        PIX_ID,
        "comprovante de 10/11",
        &entrada("pix-1", TEXTO_PIX),
        Esperado::Reconhece {
            tipo_documental: PIX_TIPO.to_owned(),
            campos: BTreeMap::from([
                ("valor_centavos".to_owned(), Valor::Centavos(15_000)),
                ("data".to_owned(), Valor::Data("2026-11-10".to_owned())),
            ]),
        },
    )
    .unwrap();
    let exec = ExecutorNoProcesso::novo();
    let ex: &dyn ExecutorDeScript = &exec;
    c.ativar(PIX_ID, v.versao, &Quem::Usuario, Some(ex))
        .unwrap();
}

/// Salva (sem ativar) uma versão nova no catálogo do cenário.
fn salvar(caminho: &Path, nova: NovaVersao) -> VersaoDeRegra {
    Catalogo::abrir(caminho)
        .unwrap()
        .salvar_rascunho(nova)
        .unwrap()
}

fn cenario() -> Cenario {
    let banco = banco_limpo();
    let dir = tempfile::tempdir().unwrap();
    let caminho = dir.path().join("regras.sqlite");
    semear(&caminho);
    let nucleo = Nucleo::novo(&banco.caminho, None)
        .unwrap()
        .com_regras(caminho.clone(), Arc::new(ExecutorNoProcesso::novo()));
    Cenario {
        _banco: banco,
        _dir: dir,
        caminho,
        nucleo,
    }
}

impl Cenario {
    fn regras(&self) -> &Regras {
        self.nucleo.regras().unwrap()
    }

    fn versoes_de(&self, regra: &str) -> usize {
        Catalogo::abrir(&self.caminho)
            .unwrap()
            .versoes(regra)
            .unwrap()
            .len()
    }

    fn saidas(&self, destinatario: &str) -> i64 {
        self._banco
            .conexao()
            .query_row(
                "SELECT COUNT(*) FROM saida_ia WHERE destinatario = ?1 AND nivel = 'mascarado'",
                [destinatario],
                |l| l.get(0),
            )
            .unwrap()
    }
}

fn pedido_de_simulacao_versao(versao: u32, entradas: Vec<Value>) -> PedidoDeSimulacao {
    PedidoDeSimulacao {
        regra: Some(PIX_ID.to_owned()),
        versao: Some(versao),
        definicao: None,
        script: None,
        entradas,
    }
}

fn entrada_json(id: &str, texto: &str) -> Value {
    serde_json::to_value(entrada(id, texto)).unwrap()
}

// ------------------------------------------------------------------------ as ferramentas

#[test]
fn listar_mostra_a_vigente_e_nao_consolida_nada() {
    let c = cenario();
    let r = c.regras().listar().unwrap();
    let item = &r["itens"][0];
    assert_eq!(item["id"], PIX_ID);
    assert_eq!(item["versao_em_vigor"], 1);
    assert_eq!(item["estado"], "ativa");
    assert_eq!(r["sugestoes_de_consolidacao"], json!([]));
    assert!(r["mensagem"].as_str().unwrap().contains("não consolida"));
}

#[test]
fn explicar_traz_versoes_definicao_e_exemplos_sem_o_texto_do_documento() {
    let c = cenario();
    let r = c
        .regras()
        .explicar(&PedidoDeRegra {
            regra: PIX_ID.to_owned(),
        })
        .unwrap();
    assert_eq!(r["versoes"].as_array().unwrap().len(), 1);
    assert_eq!(r["versoes"][0]["estado"], "ativa");
    assert_eq!(r["em_vigor"]["versao"], 1);
    assert_eq!(r["ultima"]["definicao"]["tipo_documental"], PIX_TIPO);
    assert_eq!(r["exemplos"][0]["nome"], "comprovante de 10/11");
    assert_eq!(r["exemplos"][0]["esperado"]["esperado"], "reconhece");
    assert!(r["exemplos"][0].get("entrada").is_none());

    let falta = c.regras().explicar(&PedidoDeRegra {
        regra: "nao-existe".to_owned(),
    });
    assert!(falta.unwrap_err().contains("não achei a regra"));
}

#[test]
fn testar_diz_o_que_passou_e_o_que_falhou() {
    let c = cenario();
    // A versão 2 procura um "Total" que o comprovante não tem: o exemplo falha.
    let mut quebrada = def_pix_json();
    quebrada["extrair"]["valor_centavos"]["regex"] = json!(r"Total:\s*R\$\s*([\d.,]+)");
    salvar(
        &c.caminho,
        nova(
            PIX_ID,
            PIX_NOME,
            serde_json::from_value(quebrada).unwrap(),
            None,
        ),
    );

    let ok = c
        .regras()
        .testar(&PedidoDeTeste {
            regra: PIX_ID.to_owned(),
            versao: 1,
        })
        .unwrap();
    assert_eq!(ok["aprovado"], true);
    assert_eq!(ok["falharam"], json!([]));

    let ruim = c
        .regras()
        .testar(&PedidoDeTeste {
            regra: PIX_ID.to_owned(),
            versao: 2,
        })
        .unwrap();
    assert_eq!(ruim["aprovado"], false);
    assert_eq!(ruim["falharam"], json!(["comprovante de 10/11"]));
    assert_eq!(ruim["resultados"][0]["passou"], false);
    assert!(
        ruim["mensagem"]
            .as_str()
            .unwrap()
            .contains("não pode ser ativada")
    );

    assert!(
        c.regras()
            .testar(&PedidoDeTeste {
                regra: PIX_ID.to_owned(),
                versao: 9,
            })
            .is_err()
    );
}

#[test]
fn simular_versao_salva_mostra_a_diferenca_sem_gravar() {
    let c = cenario();
    salvar(
        &c.caminho,
        nova(
            PIX_ID,
            PIX_NOME,
            serde_json::from_value(def_pix_exige_autenticacao()).unwrap(),
            None,
        ),
    );
    let antes = c.versoes_de(PIX_ID);

    let r = c
        .regras()
        .simular(&pedido_de_simulacao_versao(
            2,
            vec![
                entrada_json("pix-1", TEXTO_PIX),
                entrada_json("pix-2", TEXTO_PIX_SEM_AUTENTICACAO),
            ],
        ))
        .unwrap();
    assert_eq!(r["grava"], false);
    assert_eq!(r["iguais"], 1);
    assert_eq!(r["mudaram"], 1);
    assert_eq!(r["mudancas"][0]["entrada"], "pix-2");
    assert_eq!(r["mudancas"][0]["antes"]["decisao"], "reconhecido");
    // Campo obrigatório ausente não descarta a leitura: vai para revisão.
    assert_eq!(r["mudancas"][0]["depois"]["decisao"], "reconhecido");
    assert_eq!(r["mudancas"][0]["depois"]["revisar"], true);

    assert_eq!(c.versoes_de(PIX_ID), antes, "simular não salva versão");
    assert_eq!(
        c.regras().listar().unwrap()["itens"][0]["versao_em_vigor"],
        1
    );
}

#[test]
fn simular_definicao_avulsa_nao_salva_nem_cria_regra() {
    let c = cenario();
    let recibo = json!({
        "versao_do_esquema": 1,
        "tipo_documental": "recibo-de-teste",
        "detectar": { "todos": [{ "contem": "recibo de teste" }] }
    });
    let r = c
        .regras()
        .simular(&PedidoDeSimulacao {
            regra: None,
            versao: None,
            definicao: Some(recibo),
            script: None,
            entradas: vec![entrada_json("rec-1", "Recibo de teste\nValor: R$ 5,00")],
        })
        .unwrap();
    assert_eq!(r["mudaram"], 1);
    assert_eq!(r["mudancas"][0]["depois"]["decisao"], "reconhecido");
    assert_eq!(c.regras().listar().unwrap()["total"], 1);
    assert_eq!(c.versoes_de(PIX_ID), 1);
}

#[test]
fn simular_mascara_texto_pessoal_e_deixa_as_datas_legiveis() {
    let c = cenario();
    let mut def = json!({
        "versao_do_esquema": 1,
        "tipo_documental": "recibo-com-cpf",
        "detectar": { "todos": [{ "contem": "recibo cpf" }] },
        "extrair": {
            "documento": { "regex": r"CPF:\s*([\d.\-]+)", "formato": "texto" },
            "data": { "regex": r"Data:\s*(\d{2}/\d{2}/\d{4})", "formato": "data" }
        }
    });
    def["obrigatorios"] = json!([]);
    let r = c
        .regras()
        .simular(&PedidoDeSimulacao {
            regra: None,
            versao: None,
            definicao: Some(def),
            script: None,
            entradas: vec![entrada_json(
                "cpf-1",
                "Recibo CPF\nCPF: 123.456.789-00\nData: 10/11/2026",
            )],
        })
        .unwrap();
    let campos = &r["mudancas"][0]["depois"]["campos"];
    assert_eq!(campos["documento"]["valor"], "•••");
    assert_eq!(campos["data"]["valor"], "2026-11-10");
}

#[test]
fn simular_recusa_pedido_incompleto_e_entrada_invalida() {
    let c = cenario();
    let sem_nada = PedidoDeSimulacao {
        regra: None,
        versao: None,
        definicao: None,
        script: None,
        entradas: vec![entrada_json("x", "x")],
    };
    assert!(
        c.regras()
            .simular(&sem_nada)
            .unwrap_err()
            .contains("`regra` e `versao`")
    );

    let invalida = pedido_de_simulacao_versao(1, vec![json!({"nao": "e uma entrada"})]);
    assert!(
        c.regras()
            .simular(&invalida)
            .unwrap_err()
            .contains("entrada 1 inválida")
    );

    let vazia = pedido_de_simulacao_versao(1, vec![]);
    assert!(c.regras().simular(&vazia).is_err());
}

#[test]
fn propor_cria_rascunho_com_autor_ia_e_nao_muda_o_que_esta_em_vigor() {
    let c = cenario();
    let r = c
        .regras()
        .propor(&PedidoDeProposta {
            regra: PIX_ID.to_owned(),
            nome: PIX_NOME.to_owned(),
            definicao: def_pix_exige_autenticacao(),
            script: None,
            motivo: "o banco passou a imprimir a autenticação".to_owned(),
        })
        .unwrap();
    assert_eq!(r["criou"], true);
    assert_eq!(
        r["mensagem"],
        "rascunho criado; ativar é só na janela (área Regras de reconhecimento)"
    );
    assert_eq!(r["versao"]["versao"], 2);
    assert_eq!(r["versao"]["estado"], "rascunho");
    assert_eq!(r["versao"]["autor"], "ia");
    assert_eq!(r["em_vigor"]["versao"], 1);

    let explicada = c
        .regras()
        .explicar(&PedidoDeRegra {
            regra: PIX_ID.to_owned(),
        })
        .unwrap();
    assert_eq!(explicada["em_vigor"]["versao"], 1);
    assert_eq!(explicada["em_vigor"]["estado"], "ativa");
    assert_eq!(explicada["ultima"]["versao"], 2);
    assert_eq!(explicada["versoes"][1]["autor"], "ia");
    assert_eq!(
        explicada["versoes"][1]["motivo"],
        "o banco passou a imprimir a autenticação"
    );
}

#[test]
fn propor_o_mesmo_conteudo_nao_duplica() {
    let c = cenario();
    let pedido = PedidoDeProposta {
        regra: PIX_ID.to_owned(),
        nome: PIX_NOME.to_owned(),
        definicao: def_pix_exige_autenticacao(),
        script: None,
        motivo: "mesmo texto".to_owned(),
    };
    c.regras().propor(&pedido).unwrap();
    let de_novo = c.regras().propor(&pedido).unwrap();
    assert_eq!(de_novo["criou"], false);
    assert_eq!(de_novo["versao"]["versao"], 2);
    assert!(
        de_novo["mensagem"]
            .as_str()
            .unwrap()
            .contains("nada novo foi criado")
    );
    assert_eq!(c.versoes_de(PIX_ID), 2);
}

#[test]
fn propor_nao_renomeia_nem_troca_o_tipo_de_uma_regra_existente() {
    let c = cenario();
    let renomeada = c.regras().propor(&PedidoDeProposta {
        regra: PIX_ID.to_owned(),
        nome: "Outro nome".to_owned(),
        definicao: def_pix_exige_autenticacao(),
        script: None,
        motivo: "troca".to_owned(),
    });
    assert!(renomeada.unwrap_err().contains("outro nome ou tipo"));
    assert_eq!(c.versoes_de(PIX_ID), 1);
}

#[test]
fn propor_confere_o_script_antes_de_salvar() {
    let c = cenario();
    let ruim = c.regras().propor(&PedidoDeProposta {
        regra: PIX_ID.to_owned(),
        nome: PIX_NOME.to_owned(),
        definicao: def_pix_exige_autenticacao(),
        script: Some("function transformar(entrada, parametros, ajuda return end".to_owned()),
        motivo: "script quebrado".to_owned(),
    });
    assert!(ruim.unwrap_err().contains("não compila"));
    assert_eq!(c.versoes_de(PIX_ID), 1);
}

#[test]
fn propor_com_script_valido_cria_a_regra_como_rascunho() {
    let c = cenario();
    let script =
        "function transformar(entrada, parametros, ajuda)\n  return { corresponde = true }\nend";
    let r = c
        .regras()
        .propor(&PedidoDeProposta {
            regra: "recibo-de-teste".to_owned(),
            nome: "Recibo de teste".to_owned(),
            definicao: json!({
                "versao_do_esquema": 1,
                "tipo_documental": "recibo-de-teste",
                "detectar": { "todos": [{ "contem": "recibo de teste" }] }
            }),
            script: Some(script.to_owned()),
            motivo: "recibo fictício".to_owned(),
        })
        .unwrap();
    assert_eq!(r["criou"], true);
    assert_eq!(r["versao"]["estado"], "rascunho");
    assert_eq!(r["em_vigor"], Value::Null);
    assert_eq!(c.versoes_de("recibo-de-teste"), 1);
}

#[test]
fn d15_as_ferramentas_de_decisao_existem_e_o_propor_nao_decide() {
    let c = cenario();
    let nomes = Servidor::novo(c.nucleo.clone()).nomes_das_ferramentas();
    for f in FERRAMENTAS
        .iter()
        .chain(&sbf_mcp::regras::FERRAMENTAS_DE_ESCRITA)
    {
        assert!(nomes.iter().any(|n| n == f), "falta {f}");
    }
}

#[test]
fn sem_regras_as_ferramentas_nao_aparecem_e_as_chamadas_dizem_por_que() {
    let banco = banco_limpo();
    let nucleo = Nucleo::novo(&banco.caminho, None).unwrap();
    let nomes = Servidor::novo(nucleo.clone()).nomes_das_ferramentas();
    for f in FERRAMENTAS {
        assert!(!nomes.iter().any(|n| n == f), "não devia anunciar {f}");
    }
    assert!(nucleo.regras().unwrap_err().contains("não estão ligadas"));
}

#[test]
fn cada_chamada_grava_uma_linha_em_saida_ia_sem_conteudo() {
    let c = cenario();
    c.regras().listar().unwrap();
    c.regras()
        .explicar(&PedidoDeRegra {
            regra: PIX_ID.to_owned(),
        })
        .unwrap();
    assert_eq!(c.saidas("mcp:listar_regras"), 1);
    assert_eq!(c.saidas("mcp:explicar_regra"), 1);
}

#[test]
fn sequencia_de_chamadas_nao_muda_o_estado_da_vigente() {
    let c = cenario();
    c.regras().listar().unwrap();
    c.regras()
        .testar(&PedidoDeTeste {
            regra: PIX_ID.to_owned(),
            versao: 1,
        })
        .unwrap();
    c.regras()
        .propor(&PedidoDeProposta {
            regra: PIX_ID.to_owned(),
            nome: PIX_NOME.to_owned(),
            definicao: def_pix_exige_autenticacao(),
            script: None,
            motivo: "x".to_owned(),
        })
        .unwrap();
    let item = &c.regras().listar().unwrap()["itens"][0];
    assert_eq!(item["versao_em_vigor"], 1);
    assert_eq!(item["estado"], "ativa");
}

// ------------------------------------------------------------------------ D15: o MCP decide

fn v2_exige_autenticacao(c: &Cenario) -> u32 {
    salvar(
        &c.caminho,
        nova(
            PIX_ID,
            PIX_NOME,
            serde_json::from_value(def_pix_exige_autenticacao()).unwrap(),
            None,
        ),
    )
    .versao
}

fn estado_de(c: &Cenario, versao: u32) -> sbf_regras::Estado {
    Catalogo::abrir(&c.caminho)
        .unwrap()
        .versao(PIX_ID, versao)
        .unwrap()
        .estado
}

fn pedido_de_versao(versao: u32) -> sbf_mcp::regras::PedidoDeVersao {
    sbf_mcp::regras::PedidoDeVersao {
        regra: PIX_ID.to_owned(),
        versao,
    }
}

fn pedido_de_regra() -> PedidoDeRegra {
    PedidoDeRegra {
        regra: PIX_ID.to_owned(),
    }
}

#[test]
fn d15_ativar_regra_pelo_mcp_aplica_e_grava_quem_mcp_no_historico_de_estado() {
    let c = cenario();
    let v2 = v2_exige_autenticacao(&c);
    let r = c.regras().ativar(&pedido_de_versao(v2)).unwrap();
    assert_eq!(r["aplicado"], true);
    assert_eq!(r["em_vigor"]["versao"], v2);
    assert_eq!(estado_de(&c, v2), sbf_regras::Estado::Ativa);
    assert_eq!(estado_de(&c, 1), sbf_regras::Estado::Desativada);
    let h = Catalogo::abrir(&c.caminho)
        .unwrap()
        .historico_de_estado(PIX_ID, v2)
        .unwrap();
    assert_eq!(h.last().unwrap().2, "mcp");
    assert_eq!(c.saidas("mcp:ativar_regra"), 1);
}

#[test]
fn d12_ativar_regra_com_exemplo_falhando_continua_bloqueado_para_o_mcp() {
    let c = cenario();
    // Um exemplo que a regra não satisfaz: a versão nova não pode entrar em vigor.
    Catalogo::abrir(&c.caminho)
        .unwrap()
        .salvar_exemplo(
            PIX_ID,
            "sem autenticação",
            &entrada("pix-2", TEXTO_PIX_SEM_AUTENTICACAO),
            Esperado::NaoSeAplica,
        )
        .unwrap();
    let v2 = v2_exige_autenticacao(&c);
    let erro = c.regras().ativar(&pedido_de_versao(v2)).unwrap_err();
    assert!(erro.contains("bloqueada"), "{erro}");
    assert!(erro.contains("sem autenticação"), "{erro}");
    assert_eq!(estado_de(&c, v2), sbf_regras::Estado::Rascunho);
    assert_eq!(
        estado_de(&c, 1),
        sbf_regras::Estado::Ativa,
        "a vigente não mudou"
    );
}

#[test]
fn d15_reverter_desativar_e_reativar_pelo_mcp() {
    let c = cenario();
    let v2 = v2_exige_autenticacao(&c);
    c.regras().ativar(&pedido_de_versao(v2)).unwrap();

    let r = c.regras().reverter(&pedido_de_regra()).unwrap();
    assert_eq!(r["em_vigor"]["versao"], 1);
    assert_eq!(estado_de(&c, 1), sbf_regras::Estado::Ativa);
    assert_eq!(estado_de(&c, v2), sbf_regras::Estado::Desativada);

    let r = c.regras().desativar(&pedido_de_regra()).unwrap();
    assert!(r["em_vigor"].is_null());
    assert_eq!(estado_de(&c, 1), sbf_regras::Estado::Desativada);

    c.regras().ativar(&pedido_de_versao(1)).unwrap();
    assert_eq!(estado_de(&c, 1), sbf_regras::Estado::Ativa);
    let h = Catalogo::abrir(&c.caminho)
        .unwrap()
        .historico_de_estado(PIX_ID, 1)
        .unwrap();
    // As duas primeiras linhas são do usuário (rascunho e a ativação semeada).
    assert!(
        h.iter().skip(2).all(|(_, _, quem, _, _)| quem == "mcp"),
        "{h:?}"
    );
}

#[test]
fn d15_consolidar_regra_exige_as_aceitas_seguidas_e_depois_aplica() {
    let c = cenario();
    // Sem as 5 aceitas seguidas, o catálogo recusa (para o MCP também).
    let erro = c.regras().consolidar(&pedido_de_versao(1)).unwrap_err();
    assert!(erro.contains("faltam propostas aceitas"), "{erro}");
    assert_eq!(estado_de(&c, 1), sbf_regras::Estado::Ativa);
}

#[test]
fn d15_o_mcp_nao_ativa_o_que_nao_existe_e_a_ia_continua_recusada_no_catalogo() {
    let c = cenario();
    assert!(c.regras().ativar(&pedido_de_versao(9)).is_err());
    let mut cat = Catalogo::abrir(&c.caminho).unwrap();
    let v2 = v2_exige_autenticacao(&c);
    assert!(cat.ativar(PIX_ID, v2, &Quem::Ia, None).is_err());
}

#[test]
fn d15_primeira_escrita_de_regras_faz_backup_do_regras_sqlite_e_so_uma_vez() {
    let c = cenario();
    let pasta = c._banco.caminho.parent().unwrap().join("backups/mcp");
    let v2 = v2_exige_autenticacao(&c);
    // Leituras não copiam nada.
    c.regras().listar().unwrap();
    assert!(!pasta.exists());
    c.regras().ativar(&pedido_de_versao(v2)).unwrap();
    c.regras().reverter(&pedido_de_regra()).unwrap();
    let copias: Vec<_> = std::fs::read_dir(&pasta)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(copias.len(), 1, "{copias:?}");
    assert!(copias[0].starts_with("regras-") && copias[0].ends_with(".sqlite"));
    // A cópia é do regras.sqlite de antes da escrita: a versão 1 ainda era a ativa.
    let copia = Catalogo::abrir(&pasta.join(&copias[0])).unwrap();
    assert_eq!(
        copia.versao(PIX_ID, 1).unwrap().estado,
        sbf_regras::Estado::Ativa
    );
}

#[test]
fn d15_sem_backup_a_escrita_de_regra_e_recusada() {
    let c = cenario();
    let v2 = v2_exige_autenticacao(&c);
    // Uma FILE no lugar da pasta `backups` impede criar a pasta de backup.
    let cofre = c._banco.caminho.parent().unwrap().to_path_buf();
    std::fs::write(cofre.join("backups"), "obstáculo").unwrap();
    let erro = c.regras().ativar(&pedido_de_versao(v2)).unwrap_err();
    assert!(erro.contains("escrita foi recusada"), "{erro}");
    assert_eq!(estado_de(&c, v2), sbf_regras::Estado::Rascunho);
    let erro = c.regras().propor(&PedidoDeProposta {
        regra: "outra".into(),
        nome: "Outra".into(),
        definicao: def_pix_json(),
        script: None,
        motivo: "x".into(),
    });
    assert!(erro.is_err());
}

#[test]
fn d15_fazer_backup_pelo_nucleo_copia_o_inventario_e_o_regras() {
    let c = cenario();
    let r = c.nucleo.fazer_backup().unwrap();
    let bancos: Vec<&str> = r["feitos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["banco"].as_str().unwrap())
        .collect();
    assert!(bancos.contains(&"inventario"));
    assert!(bancos.contains(&"regras"));
}
