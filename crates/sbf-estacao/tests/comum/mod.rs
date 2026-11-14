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
    pub rascunhos: Vec<Rascunho>,
    pub lotes: Vec<LoteDeRascunhos>,
    /// `(versão dos dados, coleção, id)` de cada exclusão.
    pub exclusoes: Vec<(u64, Colecao, Id)>,
    /// Documentos recebidos: `(sha256, nome, tamanho, source_document_id, job_id)`.
    pub documentos: Vec<(String, String, usize, Id, Id)>,
    /// Derruba a conexão ao receber o documento de número N (a partir de 0) desta lista.
    pub derrubar_documento_numero: Option<usize>,
    /// Quantos pedidos de cada caminho chegaram (para provar reenvio).
    pub pedidos_de_documento: usize,
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
            contrato: 2,
            aceitos: vec![1, 2],
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
            rascunhos: Vec::new(),
            lotes: Vec::new(),
            exclusoes: Vec::new(),
            documentos: Vec::new(),
            derrubar_documento_numero: None,
            pedidos_de_documento: 0,
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
    let e = &mut *e;
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
                documentos_a_revisar: 2,
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
            if colecao.desde_a_versao() > versao {
                return erro(404, CodigoDeErro::NaoEncontrado);
            }
            if versao >= 2 {
                return delta(&e, colecao, p.valor_da_consulta("desde"));
            }
            if p.cabecalho("if-none-match") == Some(etag.as_str()) {
                return RespostaFalsa::status(304).cabecalho("etag", &etag);
            }
            let linhas = todas_as_linhas(&e, colecao);
            let resposta = json(&linhas).cabecalho("etag", &etag);
            if e.cortar == Some(colecao) {
                resposta.cortar_apos(20)
            } else {
                resposta
            }
        }
        ("POST", "/documentos") if versao >= 2 => {
            e.pedidos_de_documento += 1;
            return documento(e, p);
        }
        ("POST", "/alteracoes") => {
            let proposta: PropostaDeAlteracao = serde_json::from_slice(&p.corpo).unwrap();
            if let Some(r) = e.respostas.get(&proposta.id) {
                return json(r);
            }
            let resposta = alteracao(e, &proposta);
            e.respostas.insert(proposta.id, resposta.clone());
            json(&resposta)
        }
        _ => erro(404, CodigoDeErro::NaoEncontrado),
    }
}

pub fn conjunto<T: std::hash::Hash + Eq>(v: impl IntoIterator<Item = T>) -> HashSet<T> {
    v.into_iter().collect()
}

fn todas_as_linhas(e: &Estado_, colecao: Colecao) -> LinhasDaColecao {
    match colecao {
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
        Colecao::LotesDeRascunhos => LinhasDaColecao::LotesDeRascunhos(e.lotes.clone()),
        Colecao::Rascunhos => LinhasDaColecao::Rascunhos(e.rascunhos.clone()),
    }
}

/// O número de uma versão `v{n}`.
fn numero(versao: &Versao) -> u64 {
    versao.0.trim_start_matches('v').parse().unwrap_or(0)
}

/// O delta do fake: o cursor é `c{n}` com `n` a versão dos dados; as linhas com versão
/// maior que o cursor entram, e as exclusões também.
fn delta(e: &Estado_, colecao: Colecao, desde: Option<&str>) -> RespostaFalsa {
    let cursor = format!("c{}", e.versao_dos_dados);
    let desde_n = desde
        .and_then(|d| d.strip_prefix('c'))
        .and_then(|n| n.parse::<u64>().ok());
    let completo = desde_n.is_none();
    let depois = desde_n.unwrap_or(0);
    let linhas = match todas_as_linhas(e, colecao) {
        LinhasDaColecao::Movimentacoes(v) => LinhasDaColecao::Movimentacoes(
            v.into_iter()
                .filter(|m| completo || numero(&m.versao) > depois)
                .collect(),
        ),
        LinhasDaColecao::Rascunhos(v) => LinhasDaColecao::Rascunhos(
            v.into_iter()
                .filter(|m| completo || numero(&m.versao) > depois)
                .collect(),
        ),
        LinhasDaColecao::LotesDeRascunhos(v) => LinhasDaColecao::LotesDeRascunhos(
            v.into_iter()
                .filter(|m| completo || numero(&m.versao) > depois)
                .collect(),
        ),
        outras if completo || depois == 0 => outras,
        LinhasDaColecao::Categorias(_) => LinhasDaColecao::Categorias(vec![]),
        LinhasDaColecao::Contas(_) => LinhasDaColecao::Contas(vec![]),
        LinhasDaColecao::Instituicoes(_) => LinhasDaColecao::Instituicoes(vec![]),
        LinhasDaColecao::Cartoes(_) => LinhasDaColecao::Cartoes(vec![]),
        LinhasDaColecao::Faturas(_) => LinhasDaColecao::Faturas(vec![]),
        LinhasDaColecao::ItensDeFatura(_) => LinhasDaColecao::ItensDeFatura(vec![]),
        LinhasDaColecao::Transferencias(_) => LinhasDaColecao::Transferencias(vec![]),
        LinhasDaColecao::Rateios(_) => LinhasDaColecao::Rateios(vec![]),
        LinhasDaColecao::Tags(_) => LinhasDaColecao::Tags(vec![]),
        LinhasDaColecao::Centros(_) => LinhasDaColecao::Centros(vec![]),
        LinhasDaColecao::Fornecedores(_) => LinhasDaColecao::Fornecedores(vec![]),
        LinhasDaColecao::Apelidos(_) => LinhasDaColecao::Apelidos(vec![]),
        LinhasDaColecao::Contratos(_) => LinhasDaColecao::Contratos(vec![]),
        LinhasDaColecao::Dividas(_) => LinhasDaColecao::Dividas(vec![]),
        LinhasDaColecao::Parcelas(_) => LinhasDaColecao::Parcelas(vec![]),
        LinhasDaColecao::Recorrentes(_) => LinhasDaColecao::Recorrentes(vec![]),
        LinhasDaColecao::Ocorrencias(_) => LinhasDaColecao::Ocorrencias(vec![]),
        LinhasDaColecao::Documentos(_) => LinhasDaColecao::Documentos(vec![]),
    };
    let excluidas = e
        .exclusoes
        .iter()
        .filter(|(n, c, _)| *c == colecao && *n > depois)
        .map(|(_, _, id)| *id)
        .collect();
    let resposta = json(&DeltaDaColecao {
        cursor: Cursor(cursor),
        completo,
        linhas,
        excluidas,
    });
    if e.cortar == Some(colecao) {
        resposta.cortar_apos(20)
    } else {
        resposta
    }
}

fn recusa_de(
    proposta: &PropostaDeAlteracao,
    motivo: CodigoDeErro,
    mensagem: &str,
    agora: Vec<LinhaAgora>,
) -> RespostaDaAlteracao {
    RespostaDaAlteracao::Recusada {
        id: proposta.id,
        motivo,
        mensagem: mensagem.into(),
        sua_versao: Box::new(proposta.clone()),
        no_servidor_agora: agora,
    }
}

/// O servidor do contrato falso aplica Ações de uma movimentação ou de um rascunho; o resto
/// o teste do servidor real cobre.
fn alteracao(e: &mut Estado_, proposta: &PropostaDeAlteracao) -> RespostaDaAlteracao {
    let alvo = proposta.item.id;
    let base = proposta.versoes_de_base[0].versao.clone();
    match &proposta.acao {
        Acao::MudarCategoriaDaMovimentacao { categoria, .. } => {
            let categoria = *categoria;
            mexer_na_movimentacao(e, proposta, alvo, &base, move |m| m.categoria = categoria)
        }
        Acao::EditarMovimentacao { edicao, .. } => {
            let edicao = edicao.clone();
            mexer_na_movimentacao(e, proposta, alvo, &base, move |m| {
                m.descricao = edicao.descricao.clone();
                m.valor = edicao.valor;
                m.data = edicao.data.clone();
            })
        }
        Acao::ExcluirMovimentacao { .. } => match e.movimentacoes.iter().position(|m| m.id == alvo)
        {
            None => recusa_de(proposta, CodigoDeErro::ItemNaoEncontrado, "sumiu", vec![]),
            Some(i) if e.movimentacoes[i].versao != base => recusa_de(
                proposta,
                CodigoDeErro::VersaoDeBaseDivergente,
                "mudou",
                vec![LinhaAgora::Movimentacoes(Box::new(
                    e.movimentacoes[i].clone(),
                ))],
            ),
            Some(i) => {
                e.aplicadas += 1;
                e.versao_dos_dados += 1;
                e.movimentacoes.remove(i);
                e.exclusoes
                    .push((e.versao_dos_dados, Colecao::Movimentacoes, alvo));
                RespostaDaAlteracao::Aceita {
                    id: proposta.id,
                    linhas: vec![],
                    excluidas: vec![ItemAlterado {
                        colecao: Colecao::Movimentacoes,
                        id: alvo,
                    }],
                }
            }
        },
        Acao::AprovarRascunho { .. } => match e.rascunhos.iter().position(|r| r.id == alvo) {
            None => recusa_de(proposta, CodigoDeErro::ItemNaoEncontrado, "sumiu", vec![]),
            Some(i) if e.rascunhos[i].versao != base => recusa_de(
                proposta,
                CodigoDeErro::VersaoDeBaseDivergente,
                "mudou",
                vec![LinhaAgora::Rascunhos(Box::new(e.rascunhos[i].clone()))],
            ),
            Some(i) => {
                e.aplicadas += 1;
                e.versao_dos_dados += 1;
                let nova = Versao(format!("v{}", e.versao_dos_dados));
                let id_mov = id(7000 + e.aplicadas as u128);
                let mut mov = movimentacao(
                    7000 + e.aplicadas as u128,
                    "Lançada do rascunho",
                    1250,
                    None,
                );
                mov.versao = nova.clone();
                mov.id = id_mov;
                e.movimentacoes.push(mov.clone());
                e.rascunhos[i].estado = EstadoDoRascunho::Lancado;
                e.rascunhos[i].lancado_como = Some(id_mov);
                e.rascunhos[i].versao = nova;
                RespostaDaAlteracao::Aceita {
                    id: proposta.id,
                    linhas: vec![
                        LinhaAgora::Rascunhos(Box::new(e.rascunhos[i].clone())),
                        LinhaAgora::Movimentacoes(Box::new(mov)),
                    ],
                    excluidas: vec![],
                }
            }
        },
        _ => recusa_de(
            proposta,
            CodigoDeErro::AcaoAindaNaoAceita,
            "não neste fake",
            vec![],
        ),
    }
}

fn mexer_na_movimentacao(
    e: &mut Estado_,
    proposta: &PropostaDeAlteracao,
    alvo: Id,
    base: &Versao,
    f: impl FnOnce(&mut Movimentacao),
) -> RespostaDaAlteracao {
    match e.movimentacoes.iter().position(|m| m.id == alvo) {
        None => recusa_de(
            proposta,
            CodigoDeErro::ItemNaoEncontrado,
            "A movimentação não existe mais no servidor.",
            vec![],
        ),
        Some(i) if e.movimentacoes[i].versao != *base => recusa_de(
            proposta,
            CodigoDeErro::VersaoDeBaseDivergente,
            "A movimentação mudou no servidor depois que você alterou.",
            vec![LinhaAgora::Movimentacoes(Box::new(
                e.movimentacoes[i].clone(),
            ))],
        ),
        Some(i) => {
            e.aplicadas += 1;
            e.versao_dos_dados += 1;
            let nova = format!("v{}", e.versao_dos_dados);
            f(&mut e.movimentacoes[i]);
            e.movimentacoes[i].versao = Versao(nova);
            RespostaDaAlteracao::Aceita {
                id: proposta.id,
                linhas: vec![LinhaAgora::Movimentacoes(Box::new(
                    e.movimentacoes[i].clone(),
                ))],
                excluidas: vec![],
            }
        }
    }
}

/// O texto de um campo de um multipart.
fn campo_do_multipart(corpo: &[u8], nome: &str) -> Option<String> {
    let texto = String::from_utf8_lossy(corpo);
    let marca = format!("name=\"{nome}\"\r\n\r\n");
    let inicio = texto.find(&marca)? + marca.len();
    let fim = texto[inicio..].find("\r\n--")?;
    Some(texto[inicio..inicio + fim].to_owned())
}

fn tamanho_do_arquivo(corpo: &[u8]) -> usize {
    let marca = b"name=\"arquivo\"";
    let Some(i) = corpo.windows(marca.len()).position(|w| w == marca) else {
        return 0;
    };
    let resto = &corpo[i..];
    let Some(c) = resto.windows(4).position(|w| w == b"\r\n\r\n") else {
        return 0;
    };
    let dados = &resto[c + 4..];
    let fim = dados
        .windows(4)
        .rposition(|w| w == b"\r\n--")
        .unwrap_or(dados.len());
    fim
}

fn documento(e: &mut Estado_, p: &PedidoRecebido) -> RespostaFalsa {
    let Some(sha) = campo_do_multipart(&p.corpo, "sha256") else {
        return erro(422, CodigoDeErro::PedidoInvalido);
    };
    let nome = campo_do_multipart(&p.corpo, "nome_original").unwrap_or_default();
    let id_no_inventario = campo_do_multipart(&p.corpo, "id_no_inventario");
    if e.derrubar_documento_numero == Some(e.documentos.len()) {
        return RespostaFalsa::derrubar();
    }
    let (doc, job, ja_existia) = match e.documentos.iter().find(|d| d.0 == sha) {
        Some(d) => (d.3, d.4, true),
        None => {
            let n = e.documentos.len() as u128;
            let (doc, job) = (id(8000 + n), id(9000 + n));
            e.documentos
                .push((sha.clone(), nome, tamanho_do_arquivo(&p.corpo), doc, job));
            (doc, job, false)
        }
    };
    json(&RespostaDoDocumento {
        source_document_id: doc,
        job_id: job,
        ja_existia,
        sha256: sha,
        id_no_inventario,
    })
}

pub fn rascunho(n: u128, lote: Id, descricao: &str) -> Rascunho {
    Rascunho {
        id: id(n),
        versao: Versao("v1".into()),
        lote: Some(lote),
        documento: None,
        tipo: TipoDeRascunho::Movimentacao,
        estado: EstadoDoRascunho::PendenteDeRevisao,
        dados: serde_json::json!({
            "schema_version": 1, "draft_type": "transaction", "transaction_type": "expense",
            "amount_cents": 1250, "event_date": "2026-11-10", "description": descricao,
            "financial_product_id": id(900)
        }),
        versao_do_esquema: 1,
        correcoes: None,
        confianca_percentual: Some(90),
        conciliacao: EstadoDaConciliacao::NaoVerificado,
        movimentacao_conciliada: None,
        lancado_como: None,
        criado_em: Some("2026-11-13T12:00:00Z".into()),
    }
}

pub fn lote(n: u128, nome: &str) -> LoteDeRascunhos {
    LoteDeRascunhos {
        id: id(n),
        versao: Versao("v1".into()),
        nome: Some(nome.into()),
        estado: EstadoDoLote::Aberto,
        total: 2,
        aprovados: 0,
        recusados: 0,
        documento: None,
        criado_em: Some("2026-11-13T12:00:00Z".into()),
    }
}
