//! A fila e as regras gratuitas: IA-01 e IA-02.

mod ajuda;

use ajuda::*;
use sbf_inventario::comando::{Acao, Comandos, PedidoDeDecisao};
use sbf_inventario::{Ref, Tipo};
use sbf_triagem::Alvo;
use sbf_triagem::fila::{OpcoesDaFila, selecionar};
use sbf_triagem::sugestoes::{FiltroDeSugestoes, gravar_regras, listar_abertas};

fn decidir(banco: &Banco, acao: Acao, alvo: Ref) {
    let mut inv = banco.inventario();
    inv.executar(&PedidoDeDecisao {
        acao,
        alvos: vec![alvo],
        motivo: None,
    })
    .expect("decidir");
}

fn reg(id: i64) -> Ref {
    Ref::nova(Tipo::Registro, id.to_string())
}

#[test]
fn ia01_decididos_e_promovidos_nunca_entram() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let livre = inserir_registro(&c, &compra("2026-10-01", -1000, "COMPRA LIVRE", None));
    let decidido = inserir_registro(&c, &compra("2026-10-02", -2000, "COMPRA DECIDIDA", None));
    let promovido = inserir_registro(
        &c,
        &Reg {
            estado: "promovido",
            movimentacao_id: Some("m1"),
            ..compra("2026-10-03", -3000, "COMPRA PROMOVIDA", None)
        },
    );
    let a_livre = inserir_achado(&c, 1, "novo", Some("texto"), "livre.pdf");
    let a_decidido = inserir_achado(&c, 2, "novo", Some("texto"), "decidido.pdf");
    let a_documento = inserir_achado(&c, 3, "documento", Some("texto"), "ja-documento.pdf");
    decidir(
        &banco,
        Acao::CorrigirCategoria("Mercado".into()),
        reg(decidido),
    );
    decidir(
        &banco,
        Acao::EmDuvida,
        Ref::nova(Tipo::Achado, a_decidido.clone()),
    );

    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");

    assert!(fila.alvos.contains(&Alvo::Registro(livre)));
    assert!(fila.alvos.contains(&Alvo::Achado(a_livre)));
    assert!(
        !fila.alvos.contains(&Alvo::Registro(decidido)),
        "decisão do usuário"
    );
    assert!(
        !fila.alvos.contains(&Alvo::Registro(promovido)),
        "promovido é terminal"
    );
    assert!(
        !fila.alvos.contains(&Alvo::Achado(a_decidido)),
        "achado decidido"
    );
    assert!(
        !fila.alvos.contains(&Alvo::Achado(a_documento)),
        "achado já documento"
    );
    assert_eq!(fila.contagens.decididos_pelo_usuario, 2);
    assert_eq!(fila.contagens.promovidos, 1);
}

#[test]
fn duplicatas_por_conteudo_saem_da_fila() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let h = inserir_achado(&c, 1, "documento", Some("fatura"), "fatura.pdf");
    let doc = |descricao| Reg {
        hash: Some(h.clone()),
        ..compra("2026-10-05", -5000, descricao, None)
    };
    let primeiro = inserir_registro(&c, &doc("LINHA DA FATURA"));
    let copia = inserir_registro(&c, &doc("LINHA DA FATURA"));
    let outra = inserir_registro(&c, &doc("OUTRA LINHA"));
    // Dois achados com o mesmo fingerprint: fica o de menor hash.
    let a1 = inserir_achado(&c, 10, "novo", None, "a.pdf");
    let a2 = inserir_achado(&c, 11, "novo", None, "b.pdf");
    c.execute(
        "UPDATE achado SET canonical_fingerprint = 'fp' WHERE content_hash IN (?1, ?2)",
        [&a1, &a2],
    )
    .expect("fingerprint");

    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");

    assert!(fila.alvos.contains(&Alvo::Registro(primeiro)));
    assert!(!fila.alvos.contains(&Alvo::Registro(copia)));
    assert!(fila.alvos.contains(&Alvo::Registro(outra)));
    assert!(fila.alvos.contains(&Alvo::Achado(a1)));
    assert!(!fila.alvos.contains(&Alvo::Achado(a2)));
    assert_eq!(fila.contagens.duplicatas, 2);
}

#[test]
fn ia02_fornecedor_com_categoria_aprendida_nao_vai_a_ia() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let ensinado = inserir_registro(
        &c,
        &compra("2026-09-01", -1000, "MERCADO", Some("Mercado Delta")),
    );
    // O mesmo fornecedor, escrito de outro jeito, e o mesmo tipo.
    let novo = inserir_registro(
        &c,
        &compra("2026-10-01", -2000, "MERCADO", Some("MERCADO DELTA LTDA")),
    );
    // O mesmo fornecedor mas outro tipo: não se aplica.
    let outro_tipo = inserir_registro(
        &c,
        &Reg {
            tipo: "pagamento",
            ..compra("2026-10-02", -2500, "BOLETO", Some("Mercado Delta"))
        },
    );
    decidir(
        &banco,
        Acao::CorrigirCategoria("Mercado".into()),
        reg(ensinado),
    );

    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");

    assert!(
        !fila.alvos.contains(&Alvo::Registro(novo)),
        "resolvido sem chamada"
    );
    assert!(fila.alvos.contains(&Alvo::Registro(outro_tipo)));
    assert_eq!(fila.contagens.resolvidos_por_regra, 1);
    assert_eq!(fila.regras.len(), 1);
    assert_eq!(fila.regras[0].alvo, Alvo::Registro(novo));
    assert_eq!(fila.regras[0].valor, "Mercado");
    assert!(fila.regras[0].confianca >= 0.9);

    // Vira sugestão aberta (não decisão), e gravar de novo não duplica.
    assert_eq!(gravar_regras(&c, &fila.regras).expect("gravar"), 1);
    let fila2 = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");
    assert_eq!(gravar_regras(&c, &fila2.regras).expect("gravar"), 0);
    let abertas = listar_abertas(&c, &FiltroDeSugestoes::default()).expect("listar");
    assert_eq!(abertas.len(), 1);
    assert_eq!(abertas[0].modelo, "regra:categoria-aprendida");
    assert_eq!(
        valor_texto(
            &c,
            &format!("SELECT COALESCE(decidido_por,'') FROM registro WHERE id = {novo}")
        ),
        "pluggy@1",
        "a sugestão não decide nada"
    );
}

#[test]
fn categoria_aprendida_com_conflito_nao_vale() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let a = inserir_registro(&c, &compra("2026-09-01", -1000, "X", Some("Loja Gama")));
    let b = inserir_registro(&c, &compra("2026-09-02", -1100, "X", Some("Loja Gama")));
    let novo = inserir_registro(&c, &compra("2026-10-01", -2000, "X", Some("Loja Gama")));
    decidir(&banco, Acao::CorrigirCategoria("Casa".into()), reg(a));
    decidir(&banco, Acao::CorrigirCategoria("Lazer".into()), reg(b));

    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");

    assert!(
        fila.alvos.contains(&Alvo::Registro(novo)),
        "categorias divergem: vai à IA"
    );
    assert!(fila.regras.is_empty());
}

#[test]
fn decisao_desfeita_nao_ensina() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let a = inserir_registro(&c, &compra("2026-09-01", -1000, "X", Some("Loja Gama")));
    let novo = inserir_registro(&c, &compra("2026-10-01", -2000, "X", Some("Loja Gama")));
    let mut inv = banco.inventario();
    let ids = inv
        .executar(&PedidoDeDecisao {
            acao: Acao::CorrigirCategoria("Casa".into()),
            alvos: vec![reg(a)],
            motivo: None,
        })
        .expect("decidir");
    inv.desfazer(ids[0]).expect("desfazer");
    drop(inv);

    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");

    assert!(fila.alvos.contains(&Alvo::Registro(novo)));
    assert!(fila.regras.is_empty());
}

#[test]
fn categoria_aceita_da_ia_nao_ensina_a_regra_so_a_do_usuario() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let a = inserir_registro(&c, &compra("2026-09-01", -1000, "X", Some("Loja Gama")));
    let novo = inserir_registro(&c, &compra("2026-10-01", -2000, "X", Some("Loja Gama")));
    let mut inv = banco.inventario();
    // O usuário aceitou uma sugestão da IA (o motivo marca a origem): não é gabarito.
    inv.executar(&PedidoDeDecisao {
        acao: Acao::CorrigirCategoria("Casa".into()),
        alvos: vec![reg(a)],
        motivo: Some(sbf_triagem::aceite::MOTIVO_DA_DECISAO.to_owned()),
    })
    .expect("decidir");
    drop(inv);

    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");

    assert!(
        fila.regras.is_empty(),
        "a IA não pode se ensinar com as próprias sugestões aceitas"
    );
    assert!(fila.alvos.contains(&Alvo::Registro(novo)));
}

#[test]
fn par_de_conciliacao_ja_sugerido_fica_de_fora() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let h = inserir_achado(&c, 1, "documento", Some("recibo"), "recibo.pdf");
    let extrato = inserir_registro(
        &c,
        &Reg {
            tipo: "debito",
            conta: Some("Banco Alfa"),
            ..compra("2026-10-20", -77701, "COMPRA NO EXTRATO", None)
        },
    );
    let documento = inserir_registro(
        &c,
        &Reg {
            hash: Some(h),
            conta: Some("Banco Alfa"),
            ..compra("2026-10-21", -77701, "COMPRA NO DOCUMENTO", None)
        },
    );
    let solto = inserir_registro(&c, &compra("2026-10-22", -123, "SOLTO", None));

    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");

    assert!(!fila.alvos.contains(&Alvo::Registro(extrato)));
    assert!(!fila.alvos.contains(&Alvo::Registro(documento)));
    assert!(fila.alvos.contains(&Alvo::Registro(solto)));
    assert_eq!(fila.contagens.em_par_de_conciliacao, 2);
}

#[test]
fn registro_completo_so_entra_se_pedido_e_o_limite_vale() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let completo = inserir_registro(
        &c,
        &Reg {
            categoria: Some("Mercado"),
            conta: Some("Banco Alfa"),
            ..compra("2026-10-01", -100, "A", Some("Mercado Delta"))
        },
    );
    for i in 0..5 {
        inserir_registro(&c, &compra("2026-10-02", -200 - i, "B", None));
    }
    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");
    assert!(!fila.alvos.contains(&Alvo::Registro(completo)));
    assert_eq!(fila.contagens.completos, 1);
    let com = selecionar(
        &c,
        &OpcoesDaFila {
            incluir_completos: true,
            limite: Some(3),
            ..OpcoesDaFila::padrao()
        },
    )
    .expect("fila");
    assert_eq!(com.alvos.len(), 3);
}
