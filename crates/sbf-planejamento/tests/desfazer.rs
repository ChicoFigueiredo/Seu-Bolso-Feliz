//! Renomear e Desfazer (D15: o MCP aplica direto, e tudo se desfaz). Dados fictícios.

use sbf_planejamento::{
    Alteracao, Autor, Banco, Centavos, Certeza, Competencia, Conciliacao, Data, Escopo, Excecao,
    Fonte, Frequencia, IdOcorrencia, MudancaDeVersao, Natureza, NovoRecorrente, Planejamento,
    PlanejamentoSqlite, Prazo, SemSaldos, Versao,
};

const HOJE: Data = Data {
    ano: 2026,
    mes: 11,
    dia: 16,
};

fn comp(ano: i32, mes: u8) -> Competencia {
    Competencia { ano, mes }
}

fn servico() -> PlanejamentoSqlite {
    PlanejamentoSqlite::novo(Banco::em_memoria().expect("banco"), Box::new(SemSaldos))
}

fn criar(s: &mut PlanejamentoSqlite, nome: &str, autor: Autor) -> String {
    s.aplicar(
        Alteracao::CriarRecorrente {
            recorrente: NovoRecorrente {
                nome: nome.into(),
                grupo: "Essencial".into(),
                subgrupo: None,
                posicao: 1,
                natureza: Natureza::Saida,
                prazo: Prazo::SemFim,
                primeira_versao: Versao {
                    id: String::new(),
                    desde: comp(2026, 10),
                    valor: Some(Centavos(100_000)),
                    certeza: Certeza::Estimado,
                    fonte: Fonte::Usuario,
                    frequencia: Frequencia::Mensal { dia: 5 },
                    conta: Some("CX-0001".into()),
                    centro: None,
                    fornecedor: None,
                    categoria: None,
                    nota: None,
                },
            },
            origem: None,
        },
        autor,
    )
    .expect("criar")
    .alvo
}

fn base(s: &PlanejamentoSqlite, id: &str) -> i64 {
    s.recorrente(id)
        .expect("ler")
        .expect("existe")
        .versao_do_registro
}

fn oc(id: &str, c: Competencia) -> IdOcorrencia {
    IdOcorrencia {
        recorrente: id.into(),
        competencia: c,
        parcela: None,
    }
}

fn previsto(s: &PlanejamentoSqlite, id: &str, c: Competencia) -> Option<i64> {
    s.proximas(id, c, 1, HOJE)
        .expect("proximas")
        .first()
        .and_then(|o| o.previsto.map(|v| v.0))
}

/// A pausa e o cancelamento tiram a ocorrência da lista de próximas.
fn suprimida(s: &PlanejamentoSqlite, id: &str, c: Competencia) -> bool {
    s.proximas(id, c, 1, HOJE)
        .expect("proximas")
        .first()
        .is_none_or(|o| o.suprimida() || o.id.competencia != c)
}

fn mcp() -> Autor {
    Autor::Mcp
}

// ------------------------------------------------------------------------ renomear

#[test]
fn renomear_muda_o_nome_grava_historico_e_desfaz() {
    let mut s = servico();
    let id = criar(&mut s, "Aluguel", Autor::Usuario);
    let b = base(&s, &id);
    let recibo = s
        .aplicar(
            Alteracao::Renomear {
                recorrente: id.clone(),
                nome: "Aluguel do apartamento".into(),
                versao_base: b,
            },
            mcp(),
        )
        .expect("renomear");
    assert_eq!(
        s.recorrente(&id).unwrap().unwrap().nome,
        "Aluguel do apartamento"
    );
    assert_eq!(recibo.versao_nova, b + 1);
    let h = s.historico(Some(&id), 10).unwrap();
    assert_eq!(h[0].autor, Autor::Mcp);
    assert!(h[0].resumo.contains("Renomeou"));

    s.desfazer(&recibo.alteracao_id, Autor::Mcp)
        .expect("desfazer");
    assert_eq!(s.recorrente(&id).unwrap().unwrap().nome, "Aluguel");
}

#[test]
fn renomear_recusa_nome_vazio_e_versao_velha() {
    let mut s = servico();
    let id = criar(&mut s, "Aluguel", Autor::Usuario);
    let b = base(&s, &id);
    assert!(
        s.aplicar(
            Alteracao::Renomear {
                recorrente: id.clone(),
                nome: "  ".into(),
                versao_base: b,
            },
            mcp()
        )
        .is_err()
    );
    assert!(
        s.aplicar(
            Alteracao::Renomear {
                recorrente: id,
                nome: "Outro".into(),
                versao_base: b + 5,
            },
            mcp()
        )
        .is_err()
    );
}

// ------------------------------------------------------------------------ desfazer

#[test]
fn desfazer_criacao_exclui_logicamente() {
    let mut s = servico();
    let id = criar(&mut s, "Netflix", Autor::Mcp);
    let h = s.historico(Some(&id), 10).unwrap();
    s.desfazer(&h[0].id, Autor::Mcp).expect("desfazer");
    assert!(s.recorrente(&id).unwrap().unwrap().excluido.is_some());
}

#[test]
fn desfazer_valor_daqui_em_diante_restaura_o_anterior() {
    let mut s = servico();
    let id = criar(&mut s, "Luz", Autor::Usuario);
    let b = base(&s, &id);
    let r = s
        .aplicar(
            Alteracao::Editar {
                recorrente: id.clone(),
                escopo: Escopo::DaquiEmDiante(comp(2026, 12)),
                mudanca: MudancaDeVersao {
                    valor: Some(Some(Centavos(250_000))),
                    nota: Some(Some("subiu".into())),
                    ..MudancaDeVersao::default()
                },
                versao_base: b,
            },
            mcp(),
        )
        .unwrap();
    assert_eq!(previsto(&s, &id, comp(2026, 12)), Some(250_000));
    s.desfazer(&r.alteracao_id, Autor::Mcp).unwrap();
    assert_eq!(previsto(&s, &id, comp(2026, 12)), Some(100_000));
    assert_eq!(previsto(&s, &id, comp(2027, 3)), Some(100_000));
    let v = s.recorrente(&id).unwrap().unwrap();
    assert_eq!(v.versoes.last().unwrap().nota, None);
}

#[test]
fn desfazer_valor_so_este_mes_tira_a_excecao() {
    let mut s = servico();
    let id = criar(&mut s, "Luz", Autor::Usuario);
    let b = base(&s, &id);
    let r = s
        .aplicar(
            Alteracao::Editar {
                recorrente: id.clone(),
                escopo: Escopo::SoEsteMes(comp(2026, 12)),
                mudanca: MudancaDeVersao {
                    valor: Some(Some(Centavos(1_234))),
                    ..MudancaDeVersao::default()
                },
                versao_base: b,
            },
            mcp(),
        )
        .unwrap();
    assert_eq!(previsto(&s, &id, comp(2026, 12)), Some(1_234));
    s.desfazer(&r.alteracao_id, Autor::Mcp).unwrap();
    assert_eq!(previsto(&s, &id, comp(2026, 12)), Some(100_000));
}

#[test]
fn desfazer_pausa_e_retomada() {
    let mut s = servico();
    let id = criar(&mut s, "Academia", Autor::Usuario);
    let b = base(&s, &id);
    let pausa = s
        .aplicar(
            Alteracao::Pausar {
                recorrente: id.clone(),
                desde: comp(2026, 12),
                ate: None,
                motivo: "viagem".into(),
                versao_base: b,
            },
            mcp(),
        )
        .unwrap();
    assert!(suprimida(&s, &id, comp(2027, 1)));

    // Retomar em 2027-02, e desfazer a retomada: a pausa volta a ficar aberta.
    let b = base(&s, &id);
    let retomada = s
        .aplicar(
            Alteracao::Retomar {
                recorrente: id.clone(),
                desde: comp(2027, 2),
                versao_base: b,
            },
            mcp(),
        )
        .unwrap();
    assert!(!suprimida(&s, &id, comp(2027, 2)));
    s.desfazer(&retomada.alteracao_id, Autor::Mcp).unwrap();
    assert!(suprimida(&s, &id, comp(2027, 2)));
    assert!(suprimida(&s, &id, comp(2027, 8)));

    // Desfazer a pausa: tudo volta a ocorrer, e dá para pausar de novo.
    s.desfazer(&pausa.alteracao_id, Autor::Mcp).unwrap();
    assert!(!suprimida(&s, &id, comp(2026, 12)));
    assert!(!suprimida(&s, &id, comp(2027, 8)));
    let b = base(&s, &id);
    s.aplicar(
        Alteracao::Pausar {
            recorrente: id.clone(),
            desde: comp(2027, 1),
            ate: None,
            motivo: "outra".into(),
            versao_base: b,
        },
        mcp(),
    )
    .expect("a pausa desfeita não bloqueia uma nova");
}

#[test]
fn desfazer_cancelamento_devolve_o_anterior_ou_nenhum() {
    let mut s = servico();
    let id = criar(&mut s, "Revista", Autor::Usuario);
    let cancelar = |s: &mut PlanejamentoSqlite, desde: Competencia| {
        let b = base(s, &id);
        s.aplicar(
            Alteracao::Cancelar {
                recorrente: id.clone(),
                desde,
                motivo: "não uso".into(),
                comprovado: false,
                versao_base: b,
            },
            mcp(),
        )
        .unwrap()
    };
    let primeiro = cancelar(&mut s, comp(2027, 1));
    let segundo = cancelar(&mut s, comp(2027, 3));
    assert_eq!(
        s.recorrente(&id)
            .unwrap()
            .unwrap()
            .cancelamento
            .unwrap()
            .desde,
        comp(2027, 3)
    );
    s.desfazer(&segundo.alteracao_id, Autor::Mcp).unwrap();
    assert_eq!(
        s.recorrente(&id)
            .unwrap()
            .unwrap()
            .cancelamento
            .unwrap()
            .desde,
        comp(2027, 1)
    );
    s.desfazer(&primeiro.alteracao_id, Autor::Mcp).unwrap();
    assert!(s.recorrente(&id).unwrap().unwrap().cancelamento.is_none());
    assert!(!suprimida(&s, &id, comp(2027, 6)));
}

#[test]
fn desfazer_excecao_e_exclusao() {
    let mut s = servico();
    let id = criar(&mut s, "Seguro", Autor::Usuario);
    let r = s
        .aplicar(
            Alteracao::Excecao {
                ocorrencia: oc(&id, comp(2026, 12)),
                excecao: Excecao::Dispensar {
                    motivo: "já pago".into(),
                },
            },
            mcp(),
        )
        .unwrap();
    assert!(suprimida(&s, &id, comp(2026, 12)) || previsto(&s, &id, comp(2026, 12)).is_some());
    let dispensada = s.proximas(&id, comp(2026, 12), 1, HOJE).unwrap()[0].estado;
    s.desfazer(&r.alteracao_id, Autor::Mcp).unwrap();
    let depois = s.proximas(&id, comp(2026, 12), 1, HOJE).unwrap()[0].estado;
    assert_ne!(dispensada, depois);

    let b = base(&s, &id);
    let excl = s
        .aplicar(
            Alteracao::Excluir {
                recorrente: id.clone(),
                motivo: "engano".into(),
                versao_base: b,
            },
            mcp(),
        )
        .unwrap();
    assert!(s.recorrente(&id).unwrap().unwrap().excluido.is_some());
    s.desfazer(&excl.alteracao_id, Autor::Mcp).unwrap();
    assert!(s.recorrente(&id).unwrap().unwrap().excluido.is_none());
}

#[test]
fn desfazer_conciliacao_com_dois_movimentos_e_desconciliar() {
    let mut s = servico();
    let id = criar(&mut s, "Condomínio", Autor::Usuario);
    let o = oc(&id, comp(2026, 12));
    let conciliar = |s: &mut PlanejamentoSqlite, mov: &str, valor: i64| {
        s.aplicar(
            Alteracao::Conciliar {
                conciliacao: Conciliacao {
                    ocorrencia: o.clone(),
                    movimento: mov.into(),
                    valor: Centavos(valor),
                    data: Data {
                        ano: 2026,
                        mes: 12,
                        dia: 5,
                    },
                    confirmada: true,
                },
            },
            mcp(),
        )
        .unwrap()
    };
    let a = conciliar(&mut s, "mov-a", 60_000);
    let _b = conciliar(&mut s, "mov-b", 40_000);
    assert_eq!(s.conciliacoes_da_ocorrencia(&o).unwrap().len(), 2);

    s.desfazer(&a.alteracao_id, Autor::Mcp).unwrap();
    let restam = s.conciliacoes_da_ocorrencia(&o).unwrap();
    assert_eq!(restam.len(), 1);
    assert_eq!(restam[0].conciliacao.movimento, "mov-b");

    // Desconciliar e desfazer: a conciliação volta como estava (confirmada, mesmo valor).
    let d = s
        .aplicar(
            Alteracao::Desconciliar {
                ocorrencia: o.clone(),
                movimento: "mov-b".into(),
            },
            mcp(),
        )
        .unwrap();
    assert!(s.conciliacoes_da_ocorrencia(&o).unwrap().is_empty());
    s.desfazer(&d.alteracao_id, Autor::Mcp).unwrap();
    let volta = s.conciliacoes_da_ocorrencia(&o).unwrap();
    assert_eq!(volta.len(), 1);
    assert!(volta[0].conciliacao.confirmada);
    assert_eq!(volta[0].conciliacao.valor, Centavos(40_000));
}

// ------------------------------------------------------------------------ regras do desfazer

#[test]
fn desfazer_so_uma_vez_e_nunca_um_desfazer() {
    let mut s = servico();
    let id = criar(&mut s, "Aluguel", Autor::Usuario);
    let b = base(&s, &id);
    let r = s
        .aplicar(
            Alteracao::Renomear {
                recorrente: id.clone(),
                nome: "Novo".into(),
                versao_base: b,
            },
            mcp(),
        )
        .unwrap();
    let u = s.desfazer(&r.alteracao_id, Autor::Mcp).unwrap();
    assert!(
        s.desfazer(&r.alteracao_id, Autor::Mcp).is_err(),
        "já desfeita"
    );
    assert!(
        s.desfazer(&u.alteracao_id, Autor::Mcp).is_err(),
        "é um desfazer"
    );
    assert!(s.desfazer("hist_que_nao_existe", Autor::Mcp).is_err());
    // O Desfazer tem histórico, com o autor de quem desfez.
    let h = s.historico(Some(&id), 10).unwrap();
    assert_eq!(h[0].id, u.alteracao_id);
    assert_eq!(h[0].autor, Autor::Mcp);
}

#[test]
fn ultima_desfazivel_pula_as_desfeitas_e_as_de_outro_autor() {
    let mut s = servico();
    let id = criar(&mut s, "Aluguel", Autor::Usuario);
    assert!(s.ultima_desfazivel(&Autor::Mcp).unwrap().is_none());
    let renomear = |s: &mut PlanejamentoSqlite, nome: &str, autor: Autor| {
        let b = base(s, &id);
        s.aplicar(
            Alteracao::Renomear {
                recorrente: id.clone(),
                nome: nome.into(),
                versao_base: b,
            },
            autor,
        )
        .unwrap()
    };
    let um = renomear(&mut s, "Um", Autor::Mcp);
    let dois = renomear(&mut s, "Dois", Autor::Mcp);
    let _do_usuario = renomear(&mut s, "Do usuário", Autor::Usuario);
    assert_eq!(
        s.ultima_desfazivel(&Autor::Mcp).unwrap().unwrap().id,
        dois.alteracao_id
    );
    s.desfazer(&dois.alteracao_id, Autor::Mcp).unwrap();
    assert_eq!(
        s.ultima_desfazivel(&Autor::Mcp).unwrap().unwrap().id,
        um.alteracao_id
    );
    s.desfazer(&um.alteracao_id, Autor::Mcp).unwrap();
    assert!(s.ultima_desfazivel(&Autor::Mcp).unwrap().is_none());
}

#[test]
fn simular_nao_deixa_rastro_de_desfazer() {
    let mut s = servico();
    let id = criar(&mut s, "Aluguel", Autor::Usuario);
    let b = base(&s, &id);
    s.simular(
        &Alteracao::Renomear {
            recorrente: id.clone(),
            nome: "Só simulado".into(),
            versao_base: b,
        },
        HOJE,
    )
    .unwrap();
    assert_eq!(s.recorrente(&id).unwrap().unwrap().nome, "Aluguel");
    assert!(s.ultima_desfazivel(&Autor::Mcp).unwrap().is_none());
}

#[test]
fn limpar_um_campo_sobrevive_ao_json_da_proposta() {
    // O `Some(None)` de "limpar a nota" se perdia no JSON (virava `null`, isto é, "não muda").
    let mut s = servico();
    let id = criar(&mut s, "Aluguel", Autor::Usuario);
    let mut b = base(&s, &id);
    s.aplicar(
        Alteracao::Editar {
            recorrente: id.clone(),
            escopo: Escopo::DaquiEmDiante(comp(2026, 10)),
            mudanca: MudancaDeVersao {
                nota: Some(Some("combinado".into())),
                ..MudancaDeVersao::default()
            },
            versao_base: b,
        },
        Autor::Usuario,
    )
    .unwrap();
    b = base(&s, &id);
    let limpar = Alteracao::Editar {
        recorrente: id.clone(),
        escopo: Escopo::DaquiEmDiante(comp(2026, 10)),
        mudanca: MudancaDeVersao {
            nota: Some(None),
            conta: Some(None),
            ..MudancaDeVersao::default()
        },
        versao_base: b,
    };
    let texto = serde_json::to_string(&limpar).unwrap();
    let volta: Alteracao = serde_json::from_str(&texto).unwrap();
    assert_eq!(volta, limpar);
    let p = s.propor(limpar, "mcp", "chave-limpar").unwrap();
    s.confirmar_proposta(&p.id).unwrap();
    let v = s.recorrente(&id).unwrap().unwrap();
    assert_eq!(v.versoes[0].nota, None);
    assert_eq!(v.versoes[0].conta, None);
}
