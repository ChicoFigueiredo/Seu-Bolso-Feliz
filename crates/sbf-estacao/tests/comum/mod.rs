//! Um servidor do contrato falso, em processo (sem Postgres), sobre o servidor HTTP falso da
//! `bancada-rede`. Dados fictícios.

#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use bancada_rede::testes::{PedidoRecebido, RespostaFalsa, ServidorFalso};
use sbf_contrato::*;
use sbf_estacao::{ConfiguracaoDaEstacao, GuardaEmMemoria, Servidor};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

pub const HOST: &str = "sbf.exemplo.com.br";
pub const ID_TOKEN_BOM: &str = "id-token-ficticio-do-google";

pub fn id(n: u128) -> Id {
    Id::from_u128(n)
}

pub fn movimentacao(
    n: u128,
    descricao: &str,
    valor: Centavos,
    categoria: Option<Id>,
) -> Movimentacao {
    Movimentacao {
        id: id(n),
        versao: Versao("v1".into()),
        conta: id(900),
        tipo: TipoDeMovimentacao::Saida,
        valor,
        descricao: Some(descricao.into()),
        data: "2026-11-04".into(),
        competencia: None,
        fatura: None,
        parcela_de_divida: None,
        categoria,
        prioridade: None,
        confirmada: true,
        notas: None,
        origem: OrigemDaMovimentacao::Manual,
        ocorrencia: None,
        fornecedor: None,
        documento: None,
        tags: vec![],
        criada_em: "2026-11-04T12:00:00Z".into(),
    }
}

pub fn categoria(n: u128, nome: &str) -> Categoria {
    Categoria {
        id: id(n),
        versao: Versao("v1".into()),
        nome: nome.into(),
        pai: None,
        icone: None,
        cor: None,
        ordem: None,
        ativa: true,
    }
}

pub fn conta(n: u128, nome: &str, saldo: Centavos) -> Conta {
    Conta {
        id: id(n),
        versao: Versao("v1".into()),
        instituicao: id(800),
        nome: nome.into(),
        tipo: TipoDeConta::ContaCorrente,
        saldo_atual: Some(saldo),
        limite_de_credito: None,
        ativa: true,
        ordem: None,
    }
}

#[derive(Debug)]
pub struct Estado_ {
    pub credenciais: HashMap<String, Id>,
    pub estacoes: Vec<Estacao>,
    pub contrato: u32,
    pub aceitos: Vec<u32>,
    pub regras: String,
    pub movimentacoes: Vec<Movimentacao>,
    pub categorias: Vec<Categoria>,
    pub contas: Vec<Conta>,
    /// Sobe a cada mudança, para a ETag.
    pub versao_dos_dados: u64,
    /// Corta a resposta desta coleção no meio.
    pub cortar: Option<Colecao>,
    /// Respostas guardadas por id de alteração (idempotência).
    pub respostas: HashMap<Id, RespostaDaAlteracao>,
    /// Quantas vezes a ação foi aplicada de verdade.
    pub aplicadas: usize,
    pub revogacoes: Vec<Id>,
}

pub struct ServidorDoContrato {
    pub http: ServidorFalso,
    pub estado: Arc<Mutex<Estado_>>,
}

fn erro(status: u16, codigo: CodigoDeErro) -> RespostaFalsa {
    let corpo = serde_json::to_string(&ErroDaApi {
        codigo,
        mensagem: format!("{codigo:?}"),
        contratos_aceitos: None,
    })
    .unwrap();
    RespostaFalsa::erro_json(status, &corpo)
}

fn json<T: serde::Serialize>(valor: &T) -> RespostaFalsa {
    RespostaFalsa::json(&serde_json::to_string(valor).unwrap())
}

impl ServidorDoContrato {
    pub fn iniciar() -> Self {
        let estado = Arc::new(Mutex::new(Estado_ {
            credenciais: HashMap::new(),
            estacoes: Vec::new(),
            contrato: 1,
            aceitos: vec![1],
            regras: "0.1.0".into(),
            movimentacoes: vec![
                movimentacao(1, "Mercado Delta", 10, Some(id(51))),
                movimentacao(2, "Padaria Exemplo", 20, None),
            ],
            categorias: vec![categoria(51, "Mercado"), categoria(52, "Casa")],
            contas: vec![conta(900, "Conta Exemplo", 123_456)],
            versao_dos_dados: 1,
            cortar: None,
            respostas: HashMap::new(),
            aplicadas: 0,
            revogacoes: Vec::new(),
        }));
        let http = ServidorFalso::iniciar();
        let compartilhado = estado.clone();
        http.tratador(move |p| tratar(&compartilhado, p));
        Self { http, estado }
    }

    pub fn config(
        &self,
        pasta: &std::path::Path,
        guarda: &GuardaEmMemoria,
    ) -> ConfiguracaoDaEstacao {
        ConfiguracaoDaEstacao::nova(
            Servidor::novo(&format!("https://{HOST}")).unwrap(),
            pasta,
            Arc::new(guarda.clone()),
        )
        .rota(HOST, &self.http.base())
        .versao_das_regras("0.1.0")
        .intervalo(None)
    }

    pub fn mexer(&self, f: impl FnOnce(&mut Estado_)) {
        let mut e = self.estado.lock().unwrap();
        f(&mut e);
        e.versao_dos_dados += 1;
    }

    /// Emite uma credencial já pareada (sem passar pelo pareamento).
    pub fn credencial_pronta(&self, guarda: &GuardaEmMemoria) -> Id {
        let mut e = self.estado.lock().unwrap();
        let estacao = nova_estacao(&mut e, "Notebook de teste");
        e.credenciais.insert("sbfe_pronta".into(), estacao);
        sbf_estacao::GuardaDeCredencial::gravar(
            guarda,
            HOST,
            bancada_rede::Segredo::novo("sbfe_pronta"),
        )
        .unwrap();
        estacao
    }

    pub fn revogar(&self, estacao: Id) {
        let mut e = self.estado.lock().unwrap();
        for s in &mut e.estacoes {
            if s.id == estacao {
                s.revogada = true;
            }
        }
    }
}

fn nova_estacao(e: &mut Estado_, nome: &str) -> Id {
    let n = e.estacoes.len() as u128 + 1000;
    let estacao = Estacao {
        id: id(n),
        nome: nome.into(),
        sistema: "linux".into(),
        pareada_em: "2026-11-13T12:00:00Z".into(),
        ultimo_contato: None,
        revogada: false,
        revogada_em: None,
    };
    e.estacoes.push(estacao);
    id(n)
}

fn tratar(estado: &Mutex<Estado_>, p: &PedidoRecebido) -> RespostaFalsa {
    let mut e = estado.lock().unwrap();
    let Some(resto) = p.caminho.strip_prefix("/api/v") else {
        return RespostaFalsa::status(404);
    };
    let (versao, recurso) = resto.split_once('/').unwrap_or((resto, ""));
    let versao: u32 = versao.parse().unwrap_or(0);
    if !e.aceitos.contains(&versao) {
        let codigo = if versao < *e.aceitos.iter().min().unwrap() {
            CodigoDeErro::ContratoAntigo
        } else {
            CodigoDeErro::ContratoMaisNovo
        };
        return erro(426, codigo);
    }
    let recurso = format!("/{recurso}");
    if recurso == "/estacoes/pareamento" && p.metodo == "POST" {
        let pedido: PedidoDePareamento = serde_json::from_slice(&p.corpo).unwrap();
        if pedido.id_token != ID_TOKEN_BOM || pedido.nonce.is_empty() {
            return erro(403, CodigoDeErro::PareamentoRecusado);
        }
        let estacao = nova_estacao(&mut e, &pedido.nome);
        let credencial = format!("sbfe_credencial_ficticia_{}", e.credenciais.len());
        e.credenciais.insert(credencial.clone(), estacao);
        let estacao = e
            .estacoes
            .iter()
            .find(|s| s.id == estacao)
            .cloned()
            .unwrap();
        return json(&Pareamento {
            estacao,
            credencial,
        });
    }
    // Autenticação.
    let token = p
        .cabecalho("authorization")
        .and_then(|a| a.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_owned();
    let Some(esta) = e.credenciais.get(&token).copied() else {
        return erro(401, CodigoDeErro::NaoAutenticado);
    };
    if e.estacoes.iter().any(|s| s.id == esta && s.revogada) {
        return erro(401, CodigoDeErro::EstacaoRevogada);
    }
    let etag = format!("\"d{}\"", e.versao_dos_dados);
    match (p.metodo.as_str(), recurso.as_str()) {
        ("GET", "/estado") => {
            let estacao = e.estacoes.iter().find(|s| s.id == esta).cloned().unwrap();
            json(&Estado {
                contrato: e.contrato,
                contratos_aceitos: e.aceitos.clone(),
                regras: e.regras.clone(),
                hora_do_servidor: "2026-11-13T12:00:00Z".into(),
                dia_de_corte: 20,
                fuso: "America/Sao_Paulo".into(),
                estacao,
            })
        }
        ("GET", "/estacoes") => json(&Estacoes {
            estacoes: e.estacoes.clone(),
            esta,
        }),
        ("POST", r) if r.starts_with("/estacoes/") && r.ends_with("/revogar") => {
            let alvo: Id = r["/estacoes/".len()..r.len() - "/revogar".len()]
                .parse()
                .unwrap();
            e.revogacoes.push(alvo);
            let mut achada = None;
            for s in &mut e.estacoes {
                if s.id == alvo {
                    s.revogada = true;
                    achada = Some(s.clone());
                }
            }
            match achada {
                Some(s) => json(&s),
                None => erro(404, CodigoDeErro::NaoEncontrado),
            }
        }
        ("GET", r) if r.starts_with("/livro/") => {
            let Some(colecao) = Colecao::do_caminho(&r["/livro/".len()..]) else {
                return erro(404, CodigoDeErro::NaoEncontrado);
            };
            if p.cabecalho("if-none-match") == Some(etag.as_str()) {
                return RespostaFalsa::status(304).cabecalho("etag", &etag);
            }
            let linhas = match colecao {
                Colecao::Movimentacoes => LinhasDaColecao::Movimentacoes(e.movimentacoes.clone()),
                Colecao::Categorias => LinhasDaColecao::Categorias(e.categorias.clone()),
                Colecao::Contas => LinhasDaColecao::Contas(e.contas.clone()),
                Colecao::Instituicoes => LinhasDaColecao::Instituicoes(vec![]),
                Colecao::Cartoes => LinhasDaColecao::Cartoes(vec![]),
                Colecao::Faturas => LinhasDaColecao::Faturas(vec![]),
                Colecao::ItensDeFatura => LinhasDaColecao::ItensDeFatura(vec![]),
                Colecao::Transferencias => LinhasDaColecao::Transferencias(vec![]),
                Colecao::Rateios => LinhasDaColecao::Rateios(vec![]),
                Colecao::Tags => LinhasDaColecao::Tags(vec![]),
                Colecao::Centros => LinhasDaColecao::Centros(vec![]),
                Colecao::Fornecedores => LinhasDaColecao::Fornecedores(vec![]),
                Colecao::Apelidos => LinhasDaColecao::Apelidos(vec![]),
                Colecao::Contratos => LinhasDaColecao::Contratos(vec![]),
                Colecao::Dividas => LinhasDaColecao::Dividas(vec![]),
                Colecao::Parcelas => LinhasDaColecao::Parcelas(vec![]),
                Colecao::Recorrentes => LinhasDaColecao::Recorrentes(vec![]),
                Colecao::Ocorrencias => LinhasDaColecao::Ocorrencias(vec![]),
                Colecao::Documentos => LinhasDaColecao::Documentos(vec![]),
            };
            let resposta = json(&linhas).cabecalho("etag", &etag);
            if e.cortar == Some(colecao) {
                resposta.cortar_apos(20)
            } else {
                resposta
            }
        }
        ("POST", "/alteracoes") => {
            let proposta: PropostaDeAlteracao = serde_json::from_slice(&p.corpo).unwrap();
            if let Some(r) = e.respostas.get(&proposta.id) {
                return json(r);
            }
            let Acao::MudarCategoriaDaMovimentacao {
                movimentacao,
                categoria,
            } = proposta.acao;
            let base = proposta.versoes_de_base[0].versao.clone();
            let resposta = match e.movimentacoes.iter().position(|m| m.id == movimentacao) {
                None => RespostaDaAlteracao::Recusada {
                    id: proposta.id,
                    motivo: CodigoDeErro::ItemNaoEncontrado,
                    mensagem: "A movimentação não existe mais no servidor.".into(),
                    sua_versao: Box::new(proposta.clone()),
                    no_servidor_agora: vec![],
                },
                Some(i) if e.movimentacoes[i].versao != base => RespostaDaAlteracao::Recusada {
                    id: proposta.id,
                    motivo: CodigoDeErro::VersaoDeBaseDivergente,
                    mensagem: "A movimentação mudou no servidor depois que você alterou.".into(),
                    sua_versao: Box::new(proposta.clone()),
                    no_servidor_agora: vec![LinhaAgora::Movimentacoes(Box::new(
                        e.movimentacoes[i].clone(),
                    ))],
                },
                Some(i) => {
                    e.aplicadas += 1;
                    e.versao_dos_dados += 1;
                    let nova = format!("v{}", e.versao_dos_dados);
                    let m = &mut e.movimentacoes[i];
                    m.categoria = categoria;
                    m.versao = Versao(nova);
                    RespostaDaAlteracao::Aceita {
                        id: proposta.id,
                        linhas: vec![LinhaAgora::Movimentacoes(Box::new(m.clone()))],
                    }
                }
            };
            e.respostas.insert(proposta.id, resposta.clone());
            json(&resposta)
        }
        _ => erro(404, CodigoDeErro::NaoEncontrado),
    }
}

pub fn conjunto<T: std::hash::Hash + Eq>(v: impl IntoIterator<Item = T>) -> HashSet<T> {
    v.into_iter().collect()
}
