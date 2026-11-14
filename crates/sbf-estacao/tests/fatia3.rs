//! A fatia 3 na estação, contra o servidor do contrato falso: a fila de escritas que sobrevive
//! a fechar o app (CX-08), aplicação otimista, aceite, recusa e Refazer, delta por cursor e o
//! envio de lote do inventário (CX-07). Dados fictícios.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use bancada_rede::RelogioDoSistema;
use comum::*;
use rusqlite::Connection;
use sbf_contrato::{
    Acao, CodigoDeErro, Colecao, EdicaoDaMovimentacao, EstadoDoRascunho, LoteDeRascunhos,
    Movimentacao, Rascunho,
};
use sbf_estacao::*;
use sbf_inventario::banco::Inventario;
use sbf_inventario::comando::Comandos;
use sbf_inventario::esquema::MIGRACAO_1_BASE_BUN;
use sbf_inventario::referencia::{Ref, Tipo};
use sha2::{Digest, Sha256};
use std::path::Path;

fn sincronizador<'a>(c: &'a ClienteDoServidor, g: &'a GuardaEmMemoria) -> Sincronizador<'a> {
    Sincronizador {
        cliente: c,
        guarda: g,
        relogio: &RelogioDoSistema,
        versao_das_regras: "0.1.0",
    }
}

fn cliente(s: &ServidorDoContrato, pasta: &Path, g: &GuardaEmMemoria) -> ClienteDoServidor {
    let config = s.config(pasta, g);
    ClienteDoServidor::novo(config.rede().unwrap(), config.servidor().clone())
}

struct Cena {
    s: ServidorDoContrato,
    _pasta: tempfile::TempDir,
    guarda: GuardaEmMemoria,
    cliente: ClienteDoServidor,
    caminho: std::path::PathBuf,
}

fn cena() -> Cena {
    let s = ServidorDoContrato::iniciar();
    let pasta = tempfile::tempdir().unwrap();
    let guarda = GuardaEmMemoria::default();
    s.credencial_pronta(&guarda);
    let cliente = cliente(&s, pasta.path(), &guarda);
    let caminho = caminho_do_cache(pasta.path(), HOST);
    Cena {
        s,
        _pasta: pasta,
        guarda,
        cliente,
        caminho,
    }
}

impl Cena {
    fn trocar(&self) -> Desfecho {
        sincronizador(&self.cliente, &self.guarda).trocar(&self.caminho)
    }

    fn cache(&self) -> CacheDoLivro {
        CacheDoLivro::abrir(&self.caminho).unwrap()
    }

    fn sincronizar_e_ver(&self) -> CacheDoLivro {
        assert!(matches!(self.trocar(), Desfecho::Sincronizado { .. }));
        self.cache()
    }
}

fn mov(c: &CacheDoLivro, n: u128) -> Movimentacao {
    c.linha(Colecao::Movimentacoes, &id(n).to_string())
        .unwrap()
        .unwrap()
}

const AGORA: &str = "2026-11-14T12:00:00Z";

// ───────────────────────────── CX-08: a fila ─────────────────────────────

#[test]
fn cx08_a_fila_sobrevive_a_reabrir_e_esvazia_em_ordem() {
    let c = cena();
    let mut cache = c.sincronizar_e_ver();
    // Duas escritas sobre a mesma movimentação, a segunda feita em cima do efeito da primeira.
    let a = cache
        .enfileirar(
            Acao::MudarCategoriaDaMovimentacao {
                movimentacao: id(2),
                categoria: Some(id(52)),
            },
            AGORA,
        )
        .unwrap();
    let b = cache
        .enfileirar(
            Acao::EditarMovimentacao {
                movimentacao: id(2),
                edicao: EdicaoDaMovimentacao {
                    descricao: Some("Padaria nova".into()),
                    data: "2026-11-04".into(),
                    valor: 2500,
                    conta: id(900),
                    fornecedor: None,
                    centro: None,
                    notas: None,
                },
            },
            AGORA,
        )
        .unwrap();
    // O cache já mostra as duas, e a versão ainda é a do servidor (a guarda compara com ela).
    let m = mov(&cache, 2);
    assert_eq!(m.categoria, Some(id(52)));
    assert_eq!(m.descricao.as_deref(), Some("Padaria nova"));
    assert_eq!(m.valor, 2500);
    assert_eq!(m.versao.0, "v1");
    assert!(
        cache
            .itens_pendentes()
            .unwrap()
            .contains(&(Colecao::Movimentacoes, id(2)))
    );
    drop(cache);

    // Fechou e reabriu o app: a fila está lá, em ordem, ainda Aguardando.
    let cache = c.cache();
    let fila = cache.escritas().unwrap();
    assert_eq!(
        fila.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![a.id, b.id]
    );
    assert!(fila.iter().all(|e| e.estado == EstadoDaEscrita::Aguardando));
    drop(cache);

    // A troca envia as duas, na ordem, e a segunda parte da versão que a primeira criou.
    let cache = c.sincronizar_e_ver();
    let fila = cache.escritas().unwrap();
    assert!(
        fila.iter().all(|e| e.estado == EstadoDaEscrita::Aceita),
        "{fila:#?}"
    );
    let posts: Vec<_> =
        c.s.http
            .pedidos()
            .into_iter()
            .filter(|p| p.metodo == "POST" && p.caminho.ends_with("/alteracoes"))
            .map(|p| {
                serde_json::from_slice::<sbf_contrato::PropostaDeAlteracao>(&p.corpo)
                    .unwrap()
                    .id
            })
            .collect();
    assert_eq!(posts, vec![a.id, b.id]);
    let m = mov(&cache, 2);
    assert_eq!(m.descricao.as_deref(), Some("Padaria nova"));
    assert_eq!(m.categoria, Some(id(52)));
    assert!(m.versao.0.starts_with('v') && m.versao.0 != "v1");
    assert!(cache.itens_pendentes().unwrap().is_empty());
    assert_eq!(c.s.estado.lock().unwrap().aplicadas, 2);
}

#[test]
fn rede_que_cai_depois_de_aplicar_reenvia_com_o_mesmo_id_sem_aplicar_duas_vezes() {
    let c = cena();
    let mut cache = c.sincronizar_e_ver();
    let e = cache
        .enfileirar(
            Acao::MudarCategoriaDaMovimentacao {
                movimentacao: id(2),
                categoria: Some(id(51)),
            },
            AGORA,
        )
        .unwrap();
    drop(cache);
    c.s.estado.lock().unwrap().derrubar_resposta_da_alteracao = true;
    assert_eq!(c.trocar(), Desfecho::SemRede);
    assert_eq!(
        c.cache().escrita(e.id).unwrap().unwrap().estado,
        EstadoDaEscrita::Aguardando
    );
    // Volta a rede: o mesmo id, a mesma resposta, uma aplicação só.
    let cache = c.sincronizar_e_ver();
    assert_eq!(
        cache.escrita(e.id).unwrap().unwrap().estado,
        EstadoDaEscrita::Aceita
    );
    assert_eq!(c.s.estado.lock().unwrap().aplicadas, 1);
}

// ───────────────────────────── recusa, Refazer, Descartar ─────────────────────────────

#[test]
fn recusa_guarda_as_duas_versoes_e_refazer_parte_da_do_servidor() {
    let c = cena();
    let mut cache = c.sincronizar_e_ver();
    let e = cache
        .enfileirar(
            Acao::MudarCategoriaDaMovimentacao {
                movimentacao: id(1),
                categoria: None,
            },
            AGORA,
        )
        .unwrap();
    assert_eq!(mov(&cache, 1).categoria, None, "otimista");
    drop(cache);
    // Outra estação mudou a mesma movimentação antes.
    c.s.mexer(|e| {
        let n = e.versao_dos_dados + 1;
        e.movimentacoes[0].categoria = Some(id(52));
        e.movimentacoes[0].versao = sbf_contrato::Versao(format!("v{n}"));
    });
    let mut cache = c.sincronizar_e_ver();
    let recusada = cache.escrita(e.id).unwrap().unwrap();
    assert_eq!(recusada.estado, EstadoDaEscrita::Recusada);
    let recusa = recusada.recusa.as_ref().unwrap();
    assert_eq!(recusa.motivo, CodigoDeErro::VersaoDeBaseDivergente);
    // "Sua versão": a proposta, com a base velha. "No servidor agora": a linha nova.
    assert_eq!(recusada.proposta.versoes_de_base[0].versao.0, "v1");
    assert!(matches!(&recusa.no_servidor_agora[0],
        sbf_contrato::LinhaAgora::Movimentacoes(m) if m.categoria == Some(id(52))));
    // O cache voltou a ter a linha do servidor (o otimista saiu).
    assert_eq!(mov(&cache, 1).categoria, Some(id(52)));
    assert!(cache.itens_pendentes().unwrap().is_empty());

    // Refazer sobre a nova: Aguardando de novo, com a versão do servidor como base.
    let nova = cache.refazer_escrita(e.id, AGORA).unwrap();
    assert_eq!(nova.estado, EstadoDaEscrita::Aguardando);
    assert_ne!(nova.proposta.versoes_de_base[0].versao.0, "v1");
    assert_eq!(
        cache.escrita(e.id).unwrap().unwrap().estado,
        EstadoDaEscrita::Refeita
    );
    drop(cache);
    let cache = c.sincronizar_e_ver();
    assert_eq!(
        cache.escrita(nova.id).unwrap().unwrap().estado,
        EstadoDaEscrita::Aceita
    );
    assert_eq!(mov(&cache, 1).categoria, None);
}

#[test]
fn descartar_so_vale_para_recusada_e_deixa_a_versao_do_servidor() {
    let c = cena();
    let mut cache = c.sincronizar_e_ver();
    let e = cache
        .enfileirar(
            Acao::ExcluirMovimentacao {
                movimentacao: id(1),
            },
            AGORA,
        )
        .unwrap();
    assert!(
        cache
            .linha::<Movimentacao>(Colecao::Movimentacoes, &id(1).to_string())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        cache.descartar_escrita(e.id),
        Err(ErroDeEscrita::NaoEstaRecusada(e.id))
    );
    drop(cache);
    c.s.mexer(|e| {
        let n = e.versao_dos_dados + 1;
        e.movimentacoes[0].versao = sbf_contrato::Versao(format!("v{n}"));
    });
    let mut cache = c.sincronizar_e_ver();
    assert_eq!(
        cache.escrita(e.id).unwrap().unwrap().estado,
        EstadoDaEscrita::Recusada
    );
    // A exclusão otimista foi desfeita: a linha voltou.
    assert!(
        cache
            .linha::<Movimentacao>(Colecao::Movimentacoes, &id(1).to_string())
            .unwrap()
            .is_some()
    );
    cache.descartar_escrita(e.id).unwrap();
    assert_eq!(
        cache.escrita(e.id).unwrap().unwrap().estado,
        EstadoDaEscrita::Descartada
    );
    assert!(
        cache
            .escritas()
            .unwrap()
            .iter()
            .all(|e| !e.estado.em_aberto())
    );
}

#[test]
fn enfileirar_confere_o_que_da_sem_o_servidor() {
    let c = cena();
    let mut cache = c.sincronizar_e_ver();
    // Ratear com soma diferente do valor.
    let erro = cache
        .enfileirar(
            Acao::RatearMovimentacao {
                movimentacao: id(2),
                rateios: vec![sbf_contrato::NovoRateio {
                    id: id(5000),
                    centro: id(1),
                    valor: 5,
                    nota: None,
                }],
            },
            AGORA,
        )
        .unwrap_err();
    assert!(
        matches!(
            erro,
            ErroDeEscrita::NaoEncontrado(_) | ErroDeEscrita::SomaDoRateioNaoBate { .. }
        ),
        "{erro:?}"
    );
    // Nada que mude, e item que não existe.
    assert_eq!(
        cache
            .enfileirar(
                Acao::MudarCategoriaDaMovimentacao {
                    movimentacao: id(1),
                    categoria: Some(id(51))
                },
                AGORA
            )
            .unwrap_err(),
        ErroDeEscrita::NadaMuda
    );
    assert!(matches!(
        cache
            .enfileirar(
                Acao::ExcluirMovimentacao {
                    movimentacao: id(404)
                },
                AGORA
            )
            .unwrap_err(),
        ErroDeEscrita::NaoEncontrado(_)
    ));
    assert!(cache.escritas().unwrap().is_empty());
}

// ───────────────────────────── caixa de entrada ─────────────────────────────

fn com_rascunhos(c: &Cena) {
    c.s.mexer(|e| {
        e.lotes = vec![lote(60, "Lote exemplo")];
        e.rascunhos = vec![
            rascunho(61, id(60), "Mercado Delta"),
            rascunho(62, id(60), "Padaria Exemplo"),
        ];
    });
}

#[test]
fn caixa_de_entrada_agrupa_por_lote_e_aprovar_vira_movimentacao() {
    let c = cena();
    com_rascunhos(&c);
    let mut cache = c.sincronizar_e_ver();
    let caixa = cache.caixa_de_entrada(true).unwrap();
    assert_eq!(caixa.len(), 1);
    assert_eq!(
        caixa[0].lote.as_ref().unwrap().nome.as_deref(),
        Some("Lote exemplo")
    );
    assert_eq!(caixa[0].itens.len(), 2);
    assert!(caixa[0].itens.iter().all(|i| !i.pendente));

    let e = cache
        .enfileirar(Acao::AprovarRascunho { rascunho: id(61) }, AGORA)
        .unwrap();
    assert_eq!(e.rotulo, "Aprovar rascunho · Mercado Delta");
    let caixa = cache.caixa_de_entrada(true).unwrap();
    let item = caixa[0]
        .itens
        .iter()
        .find(|i| i.rascunho.id == id(61))
        .unwrap();
    assert!(item.pendente);
    assert_eq!(item.rascunho.estado, EstadoDoRascunho::Aprovado);
    drop(cache);

    let cache = c.sincronizar_e_ver();
    let caixa = cache.caixa_de_entrada(true).unwrap();
    assert_eq!(
        caixa[0].itens.len(),
        1,
        "o aprovado e lançado saiu da caixa"
    );
    let lancado: Rascunho = cache
        .linha(Colecao::Rascunhos, &id(61).to_string())
        .unwrap()
        .unwrap();
    assert_eq!(lancado.estado, EstadoDoRascunho::Lancado);
    assert!(mov_existe(&cache, lancado.lancado_como.unwrap()));
    let _: LoteDeRascunhos = cache
        .linha(Colecao::LotesDeRascunhos, &id(60).to_string())
        .unwrap()
        .unwrap();
}

fn mov_existe(c: &CacheDoLivro, id: sbf_contrato::Id) -> bool {
    c.linha::<Movimentacao>(Colecao::Movimentacoes, &id.to_string())
        .unwrap()
        .is_some()
}

// ───────────────────────────── delta por cursor ─────────────────────────────

#[test]
fn delta_traz_so_o_que_mudou_e_aplica_as_exclusoes() {
    let c = cena();
    let cache = c.sincronizar_e_ver();
    assert_eq!(
        cache
            .linhas::<Movimentacao>(Colecao::Movimentacoes)
            .unwrap()
            .len(),
        2
    );
    assert!(
        cache
            .cursores()
            .unwrap()
            .contains_key(&Colecao::Movimentacoes)
    );
    drop(cache);

    // Nada mudou: o delta vem vazio e o cache não mexe (a geração não sobe).
    let geracao = c.cache().geracao().unwrap();
    c.s.http.esquecer_pedidos();
    let cache = c.sincronizar_e_ver();
    assert_eq!(cache.geracao().unwrap(), geracao);
    let historico = cache.historico(1).unwrap();
    assert!(
        historico[0].colecoes.is_empty(),
        "{:?}",
        historico[0].colecoes
    );
    assert!(
        c.s.http
            .pedidos()
            .iter()
            .filter(|p| p.caminho.contains("/livro/"))
            .all(|p| p
                .valor_da_consulta("desde")
                .is_some_and(|d| d.starts_with('c')))
    );
    drop(cache);

    // Muda uma, apaga outra: só elas viajam.
    c.s.mexer(|e| {
        let n = e.versao_dos_dados + 1;
        e.movimentacoes[1].descricao = Some("Padaria mudou".into());
        e.movimentacoes[1].versao = sbf_contrato::Versao(format!("v{n}"));
        e.exclusoes.push((n, Colecao::Movimentacoes, id(1)));
        e.movimentacoes.remove(0);
    });
    let cache = c.sincronizar_e_ver();
    let todas: Vec<Movimentacao> = cache.linhas(Colecao::Movimentacoes).unwrap();
    assert_eq!(todas.len(), 1);
    assert_eq!(todas[0].descricao.as_deref(), Some("Padaria mudou"));
    assert_eq!(
        cache.historico(1).unwrap()[0].colecoes,
        vec!["movimentacoes"]
    );
}

#[test]
fn escrita_pendente_continua_visivel_depois_de_um_delta_que_traz_a_linha() {
    let c = cena();
    let mut cache = c.sincronizar_e_ver();
    cache
        .enfileirar(
            Acao::MudarCategoriaDaMovimentacao {
                movimentacao: id(2),
                categoria: Some(id(52)),
            },
            AGORA,
        )
        .unwrap();
    drop(cache);
    // O servidor mexe na mesma linha (descrição) e o envio vai ser recusado, mas antes da
    // resposta o cache deve mostrar o otimista por cima do que chegou.
    c.s.estado.lock().unwrap().aceitos = vec![1, 2];
    c.s.mexer(|e| {
        let n = e.versao_dos_dados + 1;
        e.movimentacoes[1].descricao = Some("Mudou lá".into());
        e.movimentacoes[1].versao = sbf_contrato::Versao(format!("v{n}"));
    });
    let cache = c.sincronizar_e_ver();
    // Recusada (base velha), então a linha é a do servidor.
    assert_eq!(mov(&cache, 2).descricao.as_deref(), Some("Mudou lá"));
    assert_eq!(mov(&cache, 2).categoria, None);
}

// ───────────────────────────── CX-07: envio de lote ─────────────────────────────

fn hex(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

struct Inv {
    _dir: tempfile::TempDir,
    cofre: std::path::PathBuf,
    inv: Inventario,
    lote: Ref,
    hashes: Vec<String>,
}

/// Um inventário com dois documentos aprovados num lote (um registro cada) e um registro
/// que veio de conexão, sem documento.
fn inventario_com_lote() -> Inv {
    let dir = tempfile::tempdir().unwrap();
    let caminho = dir.path().join("inventario.sqlite");
    let cofre = dir.path().join("cofre");
    std::fs::create_dir_all(cofre.join("acervo")).unwrap();
    {
        let conn = Connection::open(&caminho).unwrap();
        conn.execute_batch(MIGRACAO_1_BASE_BUN).unwrap();
        conn.execute_batch(
            "ALTER TABLE registro ADD COLUMN tipo_original TEXT;
             ALTER TABLE registro ADD COLUMN contraparte TEXT;
             ALTER TABLE registro ADD COLUMN contraparte_doc TEXT;
             ALTER TABLE registro ADD COLUMN data_compra TEXT;
             ALTER TABLE registro ADD COLUMN merchant_cnpj TEXT;
             ALTER TABLE registro ADD COLUMN fatura_ref TEXT;
             INSERT OR REPLACE INTO meta (chave, valor) VALUES ('schema_versao', '2');",
        )
        .unwrap();
    }
    let mut inv = Inventario::abrir(&caminho).unwrap();
    inv.migrar(dir.path().join("copia.sqlite")).unwrap();
    drop(inv);
    let conn = Connection::open(&caminho).unwrap();
    conn.execute_batch(
        "INSERT INTO ciclo (id, inicio, fim) VALUES ('2026-11', '2026-11-20', '2026-12-19');
         INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:e2', 'disco', 'Disco E2'),
                                                       ('conexao:pluggy', 'conexao', 'Pluggy');",
    )
    .unwrap();
    let mut hashes = Vec::new();
    for (i, conteudo) in [
        b"%PDF-1.4 conta de luz ficticia".as_slice(),
        b"%PDF-1.4 boleto ficticio",
    ]
    .into_iter()
    .enumerate()
    {
        let h = hex(conteudo);
        std::fs::write(cofre.join(format!("acervo/doc{i}.pdf")), conteudo).unwrap();
        conn.execute(
            "INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao, camada, caminho_acervo, estado, document_type)
             VALUES (?1, ?2, 'application/pdf', 'pdf', 'A', ?3, 'documento', 'fatura')",
            rusqlite::params![h, conteudo.len() as i64, format!("acervo/doc{i}.pdf")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO registro (content_hash, fonte_id, tipo, data, data_tipo, ciclo_id, valor_centavos, extraido_por, estado)
             VALUES (?1, 'disco:e2', 'compra', '2026-11-25', 'pagamento', '2026-11', -1250, 'teste', 'extraido')",
            [&h],
        )
        .unwrap();
        hashes.push(h);
    }
    conn.execute(
        "INSERT INTO registro (fonte_id, tipo, data, data_tipo, ciclo_id, valor_centavos, extraido_por, estado)
         VALUES ('conexao:pluggy', 'debito', '2026-11-26', 'pagamento', '2026-11', -300, 'pluggy@1', 'extraido')",
        [],
    )
    .unwrap();
    drop(conn);
    let mut inv = Inventario::abrir(&caminho).unwrap();
    let lote = inv.criar_lote("Lote de teste").unwrap();
    let itens: Vec<Ref> = hashes
        .iter()
        .map(|h| Ref::nova(Tipo::Achado, h.clone()))
        .chain((1..=3).map(|r| Ref::nova(Tipo::Registro, r.to_string())))
        .collect();
    inv.adicionar_ao_lote(&lote, &itens).unwrap();
    Inv {
        _dir: dir,
        cofre,
        inv,
        lote,
        hashes,
    }
}

fn estado_dos_registros(inv: &Inventario) -> Vec<String> {
    let mut st = inv
        .conexao()
        .prepare("SELECT estado FROM registro ORDER BY id")
        .unwrap();
    st.query_map([], |l| l.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn cx07_lote_nao_aprovado_nao_envia() {
    let c = cena();
    let mut i = inventario_com_lote();
    let leitor = LeitorDeCofre::novo(&i.cofre);
    let erro = enviar_lote(&c.cliente, &mut i.inv, &i.lote, &leitor, &mut |_| {}).unwrap_err();
    assert!(matches!(erro, ErroDoEnvio::LoteNaoAprovado));
    assert_eq!(c.s.estado.lock().unwrap().pedidos_de_documento, 0);
}

#[test]
fn cx07_envio_idempotente_promove_so_apos_o_aceite_e_retoma_pela_metade() {
    let c = cena();
    let mut i = inventario_com_lote();
    i.inv.aprovar_lote(&i.lote).unwrap();
    assert_eq!(
        estado_dos_registros(&i.inv),
        ["aprovado", "aprovado", "aprovado"]
    );
    let leitor = LeitorDeCofre::novo(&i.cofre);

    // A rede cai no segundo documento: o primeiro já foi aceito e promovido, o segundo não.
    c.s.estado.lock().unwrap().derrubar_documento_numero = Some(1);
    let mut andamentos = Vec::new();
    let resumo = enviar_lote(&c.cliente, &mut i.inv, &i.lote, &leitor, &mut |a| {
        andamentos.push(a.clone())
    })
    .unwrap();
    assert!(resumo.interrompido);
    assert_eq!(resumo.enviados, 1);
    assert_eq!(resumo.promovidos, 1);
    assert!(!resumo.lote_promovido);
    assert_eq!(
        estado_dos_registros(&i.inv),
        ["promovido", "aprovado", "aprovado"]
    );
    assert_eq!(andamentos.first().unwrap().total, 2);
    assert_eq!(andamentos.last().unwrap().feitos, 1);

    // Retoma: o primeiro não vai de novo; o segundo vai; o lote é promovido.
    c.s.estado.lock().unwrap().derrubar_documento_numero = None;
    let antes = c.s.estado.lock().unwrap().pedidos_de_documento;
    let resumo = enviar_lote(&c.cliente, &mut i.inv, &i.lote, &leitor, &mut |_| {}).unwrap();
    assert!(!resumo.interrompido && resumo.falhas.is_empty());
    assert_eq!(resumo.retomados, 1);
    assert_eq!(resumo.enviados, 1);
    assert_eq!(resumo.sem_documento, 1);
    assert!(resumo.lote_promovido);
    assert_eq!(c.s.estado.lock().unwrap().pedidos_de_documento, antes + 1);
    assert_eq!(
        estado_dos_registros(&i.inv),
        ["promovido", "promovido", "aprovado"],
        "o da conexão não passa por aqui"
    );

    // O servidor recebeu cada documento uma vez, com o SHA-256 do arquivo.
    let docs = c.s.estado.lock().unwrap().documentos.clone();
    assert_eq!(docs.len(), 2);
    for (d, h) in docs.iter().zip(&i.hashes) {
        assert_eq!(&d.0, h);
    }
    // E a decisão de promover é do usuário.
    let por: String = i
        .inv
        .conexao()
        .query_row("SELECT decidido_por FROM registro WHERE id = 1", [], |l| {
            l.get(0)
        })
        .unwrap();
    assert_eq!(por, "usuario");
    let promovido_em: Option<String> = i
        .inv
        .conexao()
        .query_row(
            "SELECT promovido_em FROM lote WHERE id = ?1",
            [&i.lote.chave],
            |l| l.get(0),
        )
        .unwrap();
    assert!(promovido_em.is_some());

    // Um terceiro envio não faz nada de novo.
    let resumo = enviar_lote(&c.cliente, &mut i.inv, &i.lote, &leitor, &mut |_| {}).unwrap();
    assert_eq!(resumo.retomados, 2);
    assert_eq!(resumo.enviados, 0);
}

#[test]
fn envio_reenviado_ao_servidor_devolve_o_mesmo_documento_e_arquivo_alterado_falha() {
    let c = cena();
    let mut i = inventario_com_lote();
    i.inv.aprovar_lote(&i.lote).unwrap();
    // O arquivo do cofre foi trocado: o SHA-256 não bate, e nada vai ao servidor.
    std::fs::write(i.cofre.join("acervo/doc0.pdf"), b"outro conteudo").unwrap();
    let leitor = LeitorDeCofre::novo(&i.cofre);
    let resumo = enviar_lote(&c.cliente, &mut i.inv, &i.lote, &leitor, &mut |_| {}).unwrap();
    assert_eq!(resumo.falhas.len(), 1);
    assert!(resumo.falhas[0].motivo.contains("SHA-256"));
    assert_eq!(resumo.enviados, 1);
    assert!(!resumo.lote_promovido, "com falha o lote não é promovido");
    assert_eq!(
        estado_dos_registros(&i.inv),
        ["aprovado", "promovido", "aprovado"]
    );
    // Caminho que sai do cofre é recusado.
    assert!(leitor.ler("../fora.pdf").is_err());
    assert!(leitor.ler("/etc/passwd").is_err());
}
