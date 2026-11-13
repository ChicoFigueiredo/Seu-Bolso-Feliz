//! A estação contra o servidor do contrato falso (sem Postgres): pareamento, troca com ETag,
//! corte de rede, offline, revogação, contrato e alterações. Dados fictícios.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use bancada_rede::{RelogioDoSistema, Segredo};
use comum::*;
use sbf_contrato::{Colecao, Movimentacao};
use sbf_estacao::*;
use sbf_inventario::alteracao::{
    Alteracoes, EstadoDaAlteracao, EstadoDaRede, PedidoDeAlteracao, Sincronizacao,
};
use sbf_inventario::referencia::{Ref, Tipo};
use std::time::Duration;

fn sincronizador<'a>(
    cliente: &'a ClienteDoServidor,
    guarda: &'a GuardaEmMemoria,
) -> Sincronizador<'a> {
    Sincronizador {
        cliente,
        guarda,
        relogio: &RelogioDoSistema,
        versao_das_regras: "0.1.0",
    }
}

fn cliente(
    s: &ServidorDoContrato,
    pasta: &std::path::Path,
    g: &GuardaEmMemoria,
) -> ClienteDoServidor {
    let config = s.config(pasta, g);
    ClienteDoServidor::novo(config.rede().unwrap(), config.servidor().clone())
}

fn arquivos_contem(pasta: &std::path::Path, texto: &str) -> bool {
    fn andar(p: &std::path::Path, texto: &str) -> bool {
        std::fs::read_dir(p).unwrap().any(|e| {
            let e = e.unwrap().path();
            if e.is_dir() {
                andar(&e, texto)
            } else {
                String::from_utf8_lossy(&std::fs::read(&e).unwrap()).contains(texto)
            }
        })
    }
    andar(pasta, texto)
}

/// EST-01: pareamento emite credencial, que fica só no chaveiro; login desconhecido é recusado.
#[test]
fn pareamento_guarda_a_credencial_so_no_chaveiro() {
    let s = ServidorDoContrato::iniciar();
    let pasta = tempfile::tempdir().unwrap();
    let guarda = GuardaEmMemoria::default();
    let mut estacao = EstacaoLocal::abrir(s.config(pasta.path(), &guarda)).unwrap();
    assert_eq!(estacao.estado().rede, EstadoDaRede::NaoPareada);

    let pareador = estacao.pareador();
    let recusa = pareador
        .parear_com_prova(
            &Segredo::novo("outro"),
            &Segredo::novo("n"),
            "Notebook",
            "linux",
        )
        .unwrap_err();
    assert!(matches!(recusa, ErroDePareamento::Servidor(_)));
    assert!(!guarda.tem(HOST));

    pareador
        .parear_com_prova(
            &Segredo::novo(ID_TOKEN_BOM),
            &Segredo::novo("n1"),
            "Notebook",
            "linux",
        )
        .unwrap();
    let credencial = guarda.ler(HOST).unwrap().expor().to_owned();
    assert!(credencial.starts_with("sbfe_"));
    estacao.apos_parear();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    assert_eq!(estacao.estado().rede, EstadoDaRede::Sincronizado);
    assert!(estacao.estado().copia_de.is_some());
    // Nenhum arquivo da pasta de dados (o cache incluído) tem a credencial.
    assert!(!arquivos_contem(pasta.path(), &credencial));
    // EST-19: o cache fica na pasta de dados, por servidor.
    assert!(
        pasta
            .path()
            .join("livro-caixa")
            .join(HOST)
            .join(ARQUIVO_DO_CACHE)
            .exists()
    );
    // A credencial vai no cabeçalho, e só nele.
    assert!(
        s.http
            .pedidos()
            .iter()
            .any(|p| p.cabecalho("authorization") == Some(format!("Bearer {credencial}").as_str()))
    );
}

/// EST-06 e Q5: centavos inteiros; a segunda troca usa a ETag e não muda nada.
#[test]
fn troca_com_etag_e_centavos_inteiros() {
    let s = ServidorDoContrato::iniciar();
    let pasta = tempfile::tempdir().unwrap();
    let guarda = GuardaEmMemoria::default();
    s.credencial_pronta(&guarda);
    let c = cliente(&s, pasta.path(), &guarda);
    let caminho = caminho_do_cache(pasta.path(), HOST);
    assert!(matches!(
        sincronizador(&c, &guarda).trocar(&caminho),
        Desfecho::Sincronizado { .. }
    ));
    let cache = CacheDoLivro::abrir(&caminho).unwrap();
    let movs: Vec<Movimentacao> = cache.linhas(Colecao::Movimentacoes).unwrap();
    assert_eq!(movs.iter().map(|m| m.valor).sum::<i64>(), 30); // 0,10 + 0,20
    let geracao = cache.geracao().unwrap();

    s.http.esquecer_pedidos();
    assert!(matches!(
        sincronizador(&c, &guarda).trocar(&caminho),
        Desfecho::Sincronizado { .. }
    ));
    assert_eq!(cache.geracao().unwrap(), geracao, "nada mudou");
    assert!(
        s.http
            .pedidos()
            .iter()
            .filter(|p| p.caminho.contains("/livro/"))
            .all(|p| p.cabecalho("if-none-match").is_some())
    );
    let historico = cache.historico(10).unwrap();
    assert_eq!(historico.len(), 2);
    assert!(historico[0].colecoes.is_empty());
    assert_eq!(historico[1].colecoes.len(), 19);
}

/// EST-07 e EST-08: corte no meio deixa a cópia anterior inteira; offline lê o cache.
#[test]
fn corte_de_rede_e_offline_mantem_a_copia() {
    let s = ServidorDoContrato::iniciar();
    let pasta = tempfile::tempdir().unwrap();
    let guarda = GuardaEmMemoria::default();
    s.credencial_pronta(&guarda);
    let c = cliente(&s, pasta.path(), &guarda);
    let caminho = caminho_do_cache(pasta.path(), HOST);
    sincronizador(&c, &guarda).trocar(&caminho);
    s.mexer(|e| {
        e.contas[0].saldo_atual = Some(1);
        e.movimentacoes[0].valor = 999;
        e.cortar = Some(Colecao::Movimentacoes);
    });
    assert_eq!(
        sincronizador(&c, &guarda).trocar(&caminho),
        Desfecho::SemRede
    );
    let cache = CacheDoLivro::abrir(&caminho).unwrap();
    let contas: Vec<sbf_contrato::Conta> = cache.linhas(Colecao::Contas).unwrap();
    assert_eq!(
        contas[0].saldo_atual,
        Some(123_456),
        "nem a coleção que chegou inteira entrou"
    );
    let historico = cache.historico(1).unwrap();
    assert!(historico[0].erro.is_some());

    // Servidor fora do ar: a estação mostra a cópia, com a hora dela.
    let config = s.config(pasta.path(), &guarda);
    drop(s);
    let mut estacao = EstacaoLocal::abrir(config).unwrap();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    let estado = estacao.estado();
    assert!(estado.offline());
    assert!(estado.copia_de.is_some());
    assert!(estacao.cache().is_some());
}

/// EST-02 e Q11: revogada apaga cache e credencial no primeiro contato; cofre e inventário
/// ficam; as outras estações continuam.
#[test]
fn revogacao_apaga_cache_e_credencial() {
    let s = ServidorDoContrato::iniciar();
    let pasta = tempfile::tempdir().unwrap();
    let inventario = pasta.path().join("inventario.sqlite");
    std::fs::write(&inventario, b"ficticio").unwrap();
    let guarda = GuardaEmMemoria::default();
    let esta = s.credencial_pronta(&guarda);
    let outra_guarda = GuardaEmMemoria::default();
    let outra_pasta = tempfile::tempdir().unwrap();
    let mut estacao = EstacaoLocal::abrir(s.config(pasta.path(), &guarda)).unwrap();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    assert!(estacao.cache().is_some());

    // Uma segunda estação, pareada de verdade.
    let mut outra = EstacaoLocal::abrir(s.config(outra_pasta.path(), &outra_guarda)).unwrap();
    outra
        .pareador()
        .parear_com_prova(
            &Segredo::novo(ID_TOKEN_BOM),
            &Segredo::novo("n"),
            "Desktop",
            "windows",
        )
        .unwrap();
    outra.apos_parear();
    assert!(outra.esperar_troca(Duration::from_secs(20)));

    // A primeira revoga a segunda pela área Sincronização.
    let alvo = outra.estado().esta_estacao.unwrap().alvo;
    estacao.revogar(&alvo).unwrap();
    estacao.sincronizar_agora();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    assert_eq!(estacao.estado().rede, EstadoDaRede::Sincronizado);

    outra.sincronizar_agora();
    assert!(outra.esperar_troca(Duration::from_secs(20)));
    assert_eq!(outra.estado().rede, EstadoDaRede::EstacaoRevogada);
    assert!(!outra_guarda.tem(HOST));
    assert!(outra.cache().is_none());
    assert!(!caminho_do_cache(outra_pasta.path(), HOST).exists());

    // A primeira continua; o inventário dela não foi tocado.
    estacao.sincronizar_agora();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    assert_eq!(estacao.estado().rede, EstadoDaRede::Sincronizado);
    s.revogar(esta);
    estacao.sincronizar_agora();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    assert_eq!(estacao.estado().rede, EstadoDaRede::EstacaoRevogada);
    assert_eq!(std::fs::read(&inventario).unwrap(), b"ficticio");
}

/// EST-05 e Q4: contrato recusado, servidor mais novo (só leitura) e regras mais novas.
#[test]
fn versao_do_contrato_e_das_regras() {
    let s = ServidorDoContrato::iniciar();
    let pasta = tempfile::tempdir().unwrap();
    let guarda = GuardaEmMemoria::default();
    s.credencial_pronta(&guarda);
    let caminho = caminho_do_cache(pasta.path(), HOST);
    let config = s.config(pasta.path(), &guarda);

    s.mexer(|e| e.aceitos = vec![2, 3]);
    let c = cliente(&s, pasta.path(), &guarda);
    assert_eq!(
        sincronizador(&c, &guarda).trocar(&caminho),
        Desfecho::AtualizeOApp
    );
    let futuro = ClienteDoServidor::novo(
        config.rede().unwrap(),
        config.servidor().clone().com_versao_do_contrato(9),
    );
    assert_eq!(
        sincronizador(&futuro, &guarda).trocar(&caminho),
        Desfecho::ServidorDesatualizado
    );

    s.mexer(|e| {
        e.aceitos = vec![1, 2];
        e.contrato = 2;
        e.regras = "0.2.0".into();
    });
    match sincronizador(&c, &guarda).trocar(&caminho) {
        Desfecho::Sincronizado {
            regras_mais_novas,
            so_leitura,
            ..
        } => {
            assert_eq!(regras_mais_novas.as_deref(), Some("0.2.0"));
            assert!(so_leitura);
        }
        outro => panic!("{outro:?}"),
    }
}

fn mudar(m: u128, c: Option<u128>) -> PedidoDeAlteracao {
    PedidoDeAlteracao::MudarCategoria {
        movimentacao: Ref::nova(Tipo::Movimentacao, id(m).to_string()),
        categoria: c.map(|c| Ref::nova(Tipo::Categoria, id(c).to_string())),
    }
}

/// Q6/Q7 e EST-17: propor, aceite, recusa lado a lado, refazer e descartar.
#[test]
fn alteracao_de_ponta_a_ponta() {
    let s = ServidorDoContrato::iniciar();
    let pasta = tempfile::tempdir().unwrap();
    let guarda = GuardaEmMemoria::default();
    s.credencial_pronta(&guarda);
    let mut estacao = EstacaoLocal::abrir(s.config(pasta.path(), &guarda)).unwrap();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    assert_eq!(estacao.categorias().unwrap().len(), 2);

    // Aceita.
    let a = estacao.propor(&mudar(1, Some(52))).unwrap();
    assert_eq!((a.antes.as_str(), a.depois.as_str()), ("Mercado", "Casa"));
    assert_eq!(a.estado, EstadoDaAlteracao::Aguardando);
    assert!(matches!(
        estacao.propor(&mudar(2, None)),
        Err(sbf_inventario::alteracao::ErroDeAlteracao::NadaMuda)
    ));
    estacao.sincronizar_agora();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    assert_eq!(
        estacao.alteracao(&a.id).unwrap().unwrap().estado,
        EstadoDaAlteracao::Aceita
    );
    let m: Movimentacao = estacao
        .cache()
        .unwrap()
        .linha(Colecao::Movimentacoes, &id(1).to_string())
        .unwrap()
        .unwrap();
    assert_eq!(m.categoria, Some(id(52)));
    assert_eq!(s.estado.lock().unwrap().aplicadas, 1);

    // Recusada: o servidor mudou a linha depois da base.
    let b = estacao.propor(&mudar(2, Some(51))).unwrap();
    s.mexer(|e| {
        e.movimentacoes[1].versao = sbf_contrato::Versao("v-outra".into());
        e.movimentacoes[1].descricao = Some("Padaria Exemplo (editada no web)".into());
    });
    estacao.sincronizar_agora();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    let recusada = estacao.alteracao(&b.id).unwrap().unwrap();
    assert_eq!(recusada.estado, EstadoDaAlteracao::Recusada);
    let recusa = recusada.recusa.unwrap();
    let categoria = recusa
        .campos
        .iter()
        .find(|c| c.rotulo == "Categoria")
        .unwrap();
    assert_eq!(
        (categoria.sua.as_str(), categoria.servidor.as_str()),
        ("Mercado", "Sem categoria")
    );
    assert!(
        recusa
            .campos
            .iter()
            .any(|c| c.rotulo == "Descrição" && c.difere())
    );
    assert_eq!(estacao.alteracoes(true).unwrap().len(), 1);

    // Refazer sobre a nova: aceita.
    let nova = estacao.refazer(&b.id).unwrap();
    assert_eq!(
        estacao.alteracao(&b.id).unwrap().unwrap().estado,
        EstadoDaAlteracao::Refeita
    );
    estacao.sincronizar_agora();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    assert_eq!(
        estacao.alteracao(&nova.id).unwrap().unwrap().estado,
        EstadoDaAlteracao::Aceita
    );

    // Descartar uma recusada.
    let c = estacao.propor(&mudar(1, None)).unwrap();
    s.mexer(|e| e.movimentacoes[0].versao = sbf_contrato::Versao("v-x".into()));
    estacao.sincronizar_agora();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    estacao.descartar(&c.id).unwrap();
    assert_eq!(
        estacao.alteracao(&c.id).unwrap().unwrap().estado,
        EstadoDaAlteracao::Descartada
    );
    assert!(estacao.alteracoes(true).unwrap().is_empty());
    assert!(
        estacao
            .historico()
            .unwrap()
            .iter()
            .any(|t| t.recusadas == 1)
    );
}

/// T9: a `Consulta` do livro-caixa e o `painel_da_area` sobre a cópia, com os números do
/// `sbf-livro` e todo total clicável.
#[test]
fn consulta_do_livro_sobre_o_cache() {
    use sbf_inventario::consulta::{Area, Celula, Consulta, Filtro, PedidoDeTabela};
    let s = ServidorDoContrato::iniciar();
    let pasta = tempfile::tempdir().unwrap();
    let guarda = GuardaEmMemoria::default();
    s.credencial_pronta(&guarda);
    let mut estacao = EstacaoLocal::abrir(s.config(pasta.path(), &guarda)).unwrap();
    assert!(estacao.esperar_troca(Duration::from_secs(20)));
    let consulta = ConsultaDoLivro::do_cache(estacao.cache().unwrap())
        .unwrap()
        .com_hoje(sbf_livro::Data::nova(2026, 11, 13).unwrap());

    let hoje = consulta
        .painel_da_area(Area::Hoje, &Filtro::default())
        .unwrap();
    let disponivel = &hoje.indicadores[0];
    assert_eq!(disponivel.rotulo, "Disponível agora");
    assert!(disponivel.instantaneo);
    assert!(matches!(
        disponivel.valor,
        Celula::Valor {
            centavos: 123_456,
            itens: Some(_),
            ..
        }
    ));
    assert_eq!(hoje.secoes[0].titulo, "Para resolver");
    // "Precisam de revisão" vem do /estado (documentos_a_revisar = 2).
    assert!(hoje.secoes[0].tabela.linhas.iter().any(|l| matches!(
        (&l.celulas[0], &l.celulas[1]),
        (Celula::Texto(t), Celula::Contagem { n: 2, .. }) if t == "Precisam de revisão"
    )));

    let pedido = PedidoDeTabela::Area {
        area: Area::Movimentacoes,
        filtro: Filtro::default(),
    };
    let movs = consulta.tabela(&pedido).unwrap();
    assert_eq!(movs.linhas.len(), 2);
    let contas = consulta
        .tabela(&PedidoDeTabela::Area {
            area: Area::ContasECartoes,
            filtro: Filtro::default(),
        })
        .unwrap();
    // De uma conta se chega às movimentações dela.
    let Celula::Valor {
        itens: Some(itens), ..
    } = &contas.linhas[0].celulas[3]
    else {
        panic!("saldo clicável");
    };
    assert_eq!(consulta.tabela(itens).unwrap().linhas.len(), 2);
    let m = Ref::nova(Tipo::Movimentacao, id(1).to_string());
    assert!(consulta.item(&m).unwrap().is_some());
    assert_eq!(consulta.rastro(&m).unwrap().acima[0].papel, "na conta");
}
