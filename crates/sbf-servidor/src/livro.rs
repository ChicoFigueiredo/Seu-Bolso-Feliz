//! As coleções do livro-caixa: uma consulta por coleção, **sempre** filtrada pelo dono
//! (`user_id = $1`; o Neon não tem RLS), ordenada por `id` para o `ETag` ser estável.
//! Dinheiro sai do banco como texto e vira centavos em [`crate::dinheiro`].

use sbf_contrato::*;
use tokio_postgres::{GenericClient, Row};
use uuid::Uuid;

use crate::banco::{ErroDeLeitura, Leitor, sql_data, sql_instante, sql_versao};
use crate::http::ErroHttp;
use crate::valores as v;

/// Lê uma coleção inteira do dono.
pub async fn ler_colecao<C: GenericClient>(
    cliente: &C,
    colecao: Colecao,
    dono: Uuid,
) -> Result<LinhasDaColecao, ErroHttp> {
    macro_rules! ler {
        ($sql:expr, $mapa:ident, $variante:ident) => {{
            let linhas = cliente.query($sql, &[&dono]).await?;
            LinhasDaColecao::$variante(mapear(&linhas, $mapa)?)
        }};
    }
    Ok(match colecao {
        Colecao::Instituicoes => ler!(SQL_INSTITUICOES, instituicao, Instituicoes),
        Colecao::Contas => ler!(SQL_CONTAS, conta, Contas),
        Colecao::Cartoes => ler!(SQL_CARTOES, cartao, Cartoes),
        Colecao::Faturas => ler!(SQL_FATURAS, fatura, Faturas),
        Colecao::ItensDeFatura => ler!(SQL_ITENS_DE_FATURA, item_de_fatura, ItensDeFatura),
        Colecao::Movimentacoes => {
            let sql = format!("{SQL_MOVIMENTACOES} WHERE t.user_id = $1 ORDER BY t.id");
            ler!(sql.as_str(), movimentacao, Movimentacoes)
        }
        Colecao::Transferencias => ler!(SQL_TRANSFERENCIAS, transferencia, Transferencias),
        Colecao::Rateios => ler!(SQL_RATEIOS, rateio, Rateios),
        Colecao::Categorias => ler!(SQL_CATEGORIAS, categoria, Categorias),
        Colecao::Tags => ler!(SQL_TAGS, tag, Tags),
        Colecao::Centros => ler!(SQL_CENTROS, centro, Centros),
        Colecao::Fornecedores => ler!(SQL_FORNECEDORES, fornecedor, Fornecedores),
        Colecao::Apelidos => ler!(SQL_APELIDOS, apelido, Apelidos),
        Colecao::Contratos => ler!(SQL_CONTRATOS, contrato, Contratos),
        Colecao::Dividas => ler!(SQL_DIVIDAS, divida, Dividas),
        Colecao::Parcelas => ler!(SQL_PARCELAS, parcela, Parcelas),
        Colecao::Recorrentes => ler!(SQL_RECORRENTES, recorrente, Recorrentes),
        Colecao::Ocorrencias => ler!(SQL_OCORRENCIAS, ocorrencia, Ocorrencias),
        Colecao::Documentos => ler!(SQL_DOCUMENTOS, documento, Documentos),
        Colecao::LotesDeRascunhos => ler!(SQL_LOTES, lote, LotesDeRascunhos),
        Colecao::Rascunhos => {
            let sql = format!("{SQL_RASCUNHOS} WHERE d.user_id = $1 ORDER BY d.id");
            ler!(sql.as_str(), rascunho, Rascunhos)
        }
    })
}

/// Uma movimentação do dono, como está agora (para a alteração).
pub async fn ler_movimentacao<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    id: Uuid,
) -> Result<Option<Movimentacao>, ErroHttp> {
    let sql = format!("{SQL_MOVIMENTACOES} WHERE t.user_id = $1 AND t.id = $2");
    let linha = cliente.query_opt(sql.as_str(), &[&dono, &id]).await?;
    Ok(linha.as_ref().map(movimentacao).transpose()?)
}

/// Um rascunho do dono, como está agora.
pub async fn ler_rascunho<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    id: Uuid,
) -> Result<Option<Rascunho>, ErroHttp> {
    let sql = format!("{SQL_RASCUNHOS} WHERE d.user_id = $1 AND d.id = $2");
    let linha = cliente.query_opt(sql.as_str(), &[&dono, &id]).await?;
    Ok(linha.as_ref().map(rascunho).transpose()?)
}

/// Um lote de rascunhos do dono, como está agora.
pub async fn ler_lote<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    id: Uuid,
) -> Result<Option<LoteDeRascunhos>, ErroHttp> {
    let sql = format!("{SQL_LOTES_BASE} WHERE b.user_id = $1 AND b.id = $2");
    let linha = cliente.query_opt(sql.as_str(), &[&dono, &id]).await?;
    Ok(linha.as_ref().map(lote).transpose()?)
}

/// Os rateios de uma movimentação do dono, na ordem do id.
pub async fn ler_rateios_da_movimentacao<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    movimentacao: Uuid,
) -> Result<Vec<Rateio>, ErroHttp> {
    let sql =
        format!("{SQL_RATEIOS_BASE} WHERE a.user_id = $1 AND a.transaction_id = $2 ORDER BY a.id");
    let linhas = cliente.query(sql.as_str(), &[&dono, &movimentacao]).await?;
    Ok(mapear(&linhas, rateio)?)
}

/// Rateios cujas movimentações mudaram depois de `(versao, id)`, na ordem
/// `(versao da movimentação, id do rateio)`, com a versão de cada um (para o cursor).
pub async fn ler_rateios_depois<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    depois: (i64, Uuid),
) -> Result<Vec<(i64, Rateio)>, ErroHttp> {
    let sql = format!(
        "SELECT a.id, a.transaction_id AS movimentacao, a.financial_center_id AS centro, \
         a.amount::text AS valor, a.note AS nota, {} AS criado_em, \
         (extract(epoch from t.updated_at) * 1000000)::bigint AS vt \
         FROM public.transaction_allocations a JOIN public.transactions t ON t.id = a.transaction_id \
         WHERE a.user_id = $1 AND ((extract(epoch from t.updated_at) * 1000000)::bigint, a.id) > ($2, $3) \
         ORDER BY vt, a.id",
        sql_instante!("a.created_at")
    );
    let linhas = cliente
        .query(sql.as_str(), &[&dono, &depois.0, &depois.1])
        .await?;
    linhas
        .iter()
        .map(|l| Ok((Leitor(l).valor::<i64>("vt")?, rateio(l)?)))
        .collect::<Result<_, ErroDeLeitura>>()
        .map_err(Into::into)
}

fn mapear<T>(
    linhas: &[Row],
    f: fn(&Row) -> Result<T, ErroDeLeitura>,
) -> Result<Vec<T>, ErroDeLeitura> {
    linhas.iter().map(f).collect()
}

// ───────────────────────────────── SQL ─────────────────────────────────

const SQL_INSTITUICOES: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, name AS nome, type AS tipo, color AS cor, icon_url, is_active AS ativa, ",
    "display_order AS ordem FROM public.institutions WHERE user_id = $1 ORDER BY id"
);

const SQL_CONTAS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, institution_id AS instituicao, name AS nome, type AS tipo, ",
    "current_balance::text AS saldo_atual, credit_limit::text AS limite_de_credito, ",
    "is_active AS ativa, display_order AS ordem ",
    "FROM public.financial_products WHERE user_id = $1 ORDER BY id"
);

const SQL_CARTOES: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, financial_product_id AS conta, last_four_digits AS final_do_numero, ",
    "card_brand AS bandeira, is_primary AS principal, holder_name AS titular, ",
    "credit_limit::text AS limite_de_credito, closing_day AS dia_de_fechamento, ",
    "due_day AS dia_de_vencimento, is_active AS ativo ",
    "FROM public.cards WHERE user_id = $1 ORDER BY id"
);

const SQL_FATURAS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, card_id AS cartao, ",
    sql_data!("reference_month"),
    " AS mes_de_referencia, ",
    sql_data!("cycle_start_date"),
    " AS inicio, ",
    sql_data!("cycle_end_date"),
    " AS fim, ",
    sql_data!("due_date"),
    " AS vencimento, total_amount::text AS total, paid_amount::text AS pago, status AS estado ",
    "FROM public.statement_cycles WHERE user_id = $1 ORDER BY id"
);

const SQL_ITENS_DE_FATURA: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, statement_cycle_id AS fatura, transaction_id AS movimentacao, ",
    "description AS descricao, amount::text AS valor, ",
    sql_data!("transaction_date"),
    " AS data, installment_number AS parcela, total_installments AS total_de_parcelas, ",
    "supplier_id AS fornecedor FROM public.statement_items WHERE user_id = $1 ORDER BY id"
);

/// Sem `WHERE`: quem usa acrescenta o filtro pelo dono.
const SQL_MOVIMENTACOES: &str = concat!(
    "SELECT t.id, ",
    sql_versao!("t.updated_at"),
    " AS versao, t.financial_product_id AS conta, t.type AS tipo, t.amount::text AS valor, ",
    "t.description AS descricao, ",
    sql_data!("t.event_date"),
    " AS data, ",
    sql_data!("t.competence_date"),
    " AS competencia, t.statement_cycle_id AS fatura, ",
    "t.liability_installment_id AS parcela_de_divida, t.category_id AS categoria, ",
    "t.priority AS prioridade, t.is_confirmed AS confirmada, t.notes AS notas, ",
    "t.origin_type AS origem, t.recurring_instance_id AS ocorrencia, ",
    "t.supplier_id AS fornecedor, t.source_document_id AS documento, ",
    "COALESCE((SELECT array_agg(tt.tag_id ORDER BY tt.tag_id) FROM public.transaction_tags tt ",
    "JOIN public.tags g ON g.id = tt.tag_id AND g.user_id = t.user_id ",
    "WHERE tt.transaction_id = t.id), '{}'::uuid[]) AS tags, ",
    sql_instante!("t.created_at"),
    " AS criada_em FROM public.transactions t"
);

const SQL_TRANSFERENCIAS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, source_product_id AS origem, target_product_id AS destino, ",
    "amount::text AS valor, description AS descricao, ",
    sql_data!("event_date"),
    " AS data, ",
    sql_data!("competence_date"),
    " AS competencia, is_confirmed AS confirmada, notes AS notas ",
    "FROM public.transfers WHERE user_id = $1 ORDER BY id"
);

const SQL_RATEIOS: &str = concat!(
    "SELECT id, transaction_id AS movimentacao, financial_center_id AS centro, ",
    "amount::text AS valor, note AS nota, ",
    sql_instante!("created_at"),
    " AS criado_em FROM public.transaction_allocations WHERE user_id = $1 ORDER BY id"
);

/// Sem `WHERE`.
const SQL_RATEIOS_BASE: &str = concat!(
    "SELECT a.id, a.transaction_id AS movimentacao, a.financial_center_id AS centro, ",
    "a.amount::text AS valor, a.note AS nota, ",
    sql_instante!("a.created_at"),
    " AS criado_em FROM public.transaction_allocations a"
);

/// Sem `WHERE`.
const SQL_LOTES_BASE: &str = concat!(
    "SELECT b.id, ",
    sql_versao!("b.updated_at"),
    " AS versao, b.name AS nome, b.status AS estado, COALESCE(b.total_drafts, 0) AS total, ",
    "COALESCE(b.approved_count, 0) AS aprovados, COALESCE(b.rejected_count, 0) AS recusados, ",
    "b.source_document_id AS documento, ",
    sql_instante!("b.created_at"),
    " AS criado_em FROM public.draft_batches b"
);

const SQL_LOTES: &str = concat!(
    "SELECT b.id, ",
    sql_versao!("b.updated_at"),
    " AS versao, b.name AS nome, b.status AS estado, COALESCE(b.total_drafts, 0) AS total, ",
    "COALESCE(b.approved_count, 0) AS aprovados, COALESCE(b.rejected_count, 0) AS recusados, ",
    "b.source_document_id AS documento, ",
    sql_instante!("b.created_at"),
    " AS criado_em FROM public.draft_batches b WHERE b.user_id = $1 ORDER BY b.id"
);

/// Sem `WHERE`.
const SQL_RASCUNHOS: &str = concat!(
    "SELECT d.id, ",
    sql_versao!("d.updated_at"),
    " AS versao, d.batch_id AS lote, d.source_document_id AS documento, d.draft_type AS tipo, ",
    "d.status AS estado, d.draft_data AS dados, d.draft_schema_version::int AS versao_do_esquema, ",
    "d.corrections AS correcoes, (d.confidence_score * 100)::int AS confianca, ",
    "d.reconciliation_status AS conciliacao, d.reconciled_transaction_id AS movimentacao_conciliada, ",
    "d.posted_record_id AS lancado_como, ",
    sql_instante!("d.created_at"),
    " AS criado_em FROM public.draft_records d"
);

const SQL_CATEGORIAS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, name AS nome, parent_id AS pai, icon AS icone, color AS cor, ",
    "display_order AS ordem, is_active AS ativa ",
    "FROM public.categories WHERE user_id = $1 ORDER BY id"
);

const SQL_TAGS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, name AS nome, color AS cor, influences_priority AS influencia_prioridade, ",
    "suggested_priority AS prioridade_sugerida FROM public.tags WHERE user_id = $1 ORDER BY id"
);

const SQL_CENTROS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, name AS nome, type AS tipo, description AS descricao, status AS estado, ",
    "icon AS icone, color AS cor FROM public.financial_centers WHERE user_id = $1 ORDER BY id"
);

const SQL_FORNECEDORES: &str = concat!(
    "SELECT s.id, ",
    sql_versao!("s.updated_at"),
    " AS versao, s.name AS nome, s.trade_name AS nome_fantasia, s.legal_name AS razao_social, ",
    "s.document_number AS documento, s.type AS tipo, s.website AS site, s.notes AS notas, ",
    "s.institution_id AS instituicao, s.is_active AS ativo, s.display_order AS ordem, ",
    "COALESCE((SELECT array_agg(st.tag_id ORDER BY st.tag_id) FROM public.supplier_tags st ",
    "WHERE st.supplier_id = s.id AND st.user_id = s.user_id), '{}'::uuid[]) AS tags ",
    "FROM public.suppliers s WHERE s.user_id = $1 ORDER BY s.id"
);

const SQL_APELIDOS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, supplier_id AS fornecedor, alias_name AS nome, alias_type AS tipo, ",
    "is_active AS ativo, ",
    sql_data!("valid_from"),
    " AS valido_desde, ",
    sql_data!("valid_until"),
    " AS valido_ate, notes AS notas FROM public.supplier_aliases WHERE user_id = $1 ORDER BY id"
);

const SQL_CONTRATOS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, supplier_id AS fornecedor, contract_type AS tipo, identifier AS identificador, ",
    "label AS rotulo, is_active AS ativo, ",
    sql_data!("start_date"),
    " AS inicio, ",
    sql_data!("end_date"),
    " AS fim, notes AS notas FROM public.supplier_contracts WHERE user_id = $1 ORDER BY id"
);

const SQL_DIVIDAS: &str = concat!(
    "SELECT l.id, ",
    sql_versao!("l.updated_at"),
    " AS versao, l.financial_product_id AS conta, l.name AS nome, l.type AS tipo, ",
    "l.original_amount::text AS valor_original, l.outstanding_balance::text AS saldo_devedor, ",
    "l.interest_rate::text AS taxa_de_juros, l.rate_type AS periodo_da_taxa, ",
    "l.amortization_system AS sistema_de_amortizacao, l.total_installments AS total_de_parcelas, ",
    "l.paid_installments AS parcelas_pagas, ",
    sql_data!("l.start_date"),
    " AS inicio, ",
    sql_data!("l.end_date"),
    " AS fim, l.status AS estado, l.supplier_id AS fornecedor, ",
    "COALESCE((SELECT array_agg(lt.tag_id ORDER BY lt.tag_id) FROM public.liability_tags lt ",
    "JOIN public.tags g ON g.id = lt.tag_id AND g.user_id = l.user_id ",
    "WHERE lt.liability_id = l.id), '{}'::uuid[]) AS tags ",
    "FROM public.liabilities l WHERE l.user_id = $1 ORDER BY l.id"
);

const SQL_PARCELAS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, liability_id AS divida, installment_number AS numero, ",
    sql_data!("due_date"),
    " AS vencimento, total_amount::text AS total, principal_amount::text AS principal, ",
    "interest_amount::text AS juros, insurance_amount::text AS seguro, fee_amount::text AS tarifa, ",
    "paid_amount::text AS pago, ",
    sql_data!("paid_date"),
    " AS data_do_pagamento, status AS estado ",
    "FROM public.liability_installments WHERE user_id = $1 ORDER BY id"
);

const SQL_RECORRENTES: &str = concat!(
    "SELECT r.id, ",
    sql_versao!("r.updated_at"),
    " AS versao, r.financial_product_id AS conta, r.name AS nome, r.type AS tipo, ",
    "r.amount::text AS valor, r.is_variable_amount AS valor_variavel, r.frequency AS frequencia, ",
    "r.day_of_month AS dia_do_mes, r.custom_interval_days AS intervalo_em_dias, ",
    "r.category_id AS categoria, r.priority AS prioridade, ",
    sql_data!("r.starts_at"),
    " AS inicio, ",
    sql_data!("r.ends_at"),
    " AS fim, r.is_active AS ativo, r.notes AS notas, r.supplier_id AS fornecedor, ",
    "COALESCE((SELECT array_agg(rt.tag_id ORDER BY rt.tag_id) FROM public.recurring_template_tags rt ",
    "JOIN public.tags g ON g.id = rt.tag_id AND g.user_id = r.user_id ",
    "WHERE rt.recurring_template_id = r.id), '{}'::uuid[]) AS tags ",
    "FROM public.recurring_templates r WHERE r.user_id = $1 ORDER BY r.id"
);

const SQL_OCORRENCIAS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, recurring_template_id AS recorrente, ",
    sql_data!("expected_date"),
    " AS data_prevista, expected_amount::text AS valor_previsto, actual_amount::text AS valor_real, ",
    "status AS estado, ",
    sql_data!("paid_date"),
    " AS data_do_pagamento, transaction_id AS movimentacao, notes AS notas ",
    "FROM public.recurring_instances WHERE user_id = $1 ORDER BY id"
);

const SQL_DOCUMENTOS: &str = concat!(
    "SELECT id, ",
    sql_versao!("updated_at"),
    " AS versao, origin_type AS origem, filename AS nome_do_arquivo, mime_type AS tipo_mime, ",
    "file_size_bytes AS tamanho_em_bytes, status AS estado, content_hash AS hash_do_conteudo, ",
    "document_type AS tipo_de_documento, supplier_id AS fornecedor, ",
    "supplier_name_raw AS nome_do_fornecedor_bruto, gmail_subject AS assunto_do_email, ",
    "gmail_from AS remetente_do_email, ",
    sql_instante!("gmail_date"),
    " AS recebido_em, ",
    sql_instante!("created_at"),
    " AS criado_em FROM public.source_documents WHERE user_id = $1 ORDER BY id"
);

// ─────────────────────────────── mapeamento ───────────────────────────────

fn instituicao(r: &Row) -> Result<Instituicao, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Instituicao {
        id: l.id("id")?,
        versao: l.versao()?,
        nome: l.texto("nome")?,
        tipo: l.fechado("tipo", v::tipo_de_instituicao)?,
        cor: l.texto_opc("cor")?,
        icone_url: l.texto_opc("icon_url")?,
        ativa: l.booleano("ativa")?,
        ordem: l.inteiro_opc("ordem")?,
    })
}

fn conta(r: &Row) -> Result<Conta, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Conta {
        id: l.id("id")?,
        versao: l.versao()?,
        instituicao: l.id("instituicao")?,
        nome: l.texto("nome")?,
        tipo: l.fechado("tipo", v::tipo_de_conta)?,
        saldo_atual: l.centavos_opc("saldo_atual")?,
        limite_de_credito: l.centavos_opc("limite_de_credito")?,
        ativa: l.booleano("ativa")?,
        ordem: l.inteiro_opc("ordem")?,
    })
}

fn cartao(r: &Row) -> Result<Cartao, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Cartao {
        id: l.id("id")?,
        versao: l.versao()?,
        conta: l.id("conta")?,
        final_do_numero: l.texto_opc("final_do_numero")?,
        bandeira: l.texto_opc("bandeira")?,
        principal: l.booleano("principal")?,
        titular: l.texto_opc("titular")?,
        limite_de_credito: l.centavos_opc("limite_de_credito")?,
        dia_de_fechamento: l.inteiro_opc("dia_de_fechamento")?,
        dia_de_vencimento: l.inteiro_opc("dia_de_vencimento")?,
        ativo: l.booleano("ativo")?,
    })
}

fn fatura(r: &Row) -> Result<Fatura, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Fatura {
        id: l.id("id")?,
        versao: l.versao()?,
        cartao: l.id("cartao")?,
        mes_de_referencia: l.texto("mes_de_referencia")?,
        inicio: l.texto("inicio")?,
        fim: l.texto("fim")?,
        vencimento: l.texto("vencimento")?,
        total: l.centavos_opc("total")?,
        pago: l.centavos_opc("pago")?,
        estado: l.fechado("estado", v::estado_da_fatura)?,
    })
}

fn item_de_fatura(r: &Row) -> Result<ItemDeFatura, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(ItemDeFatura {
        id: l.id("id")?,
        versao: l.versao()?,
        fatura: l.id("fatura")?,
        movimentacao: l.id_opc("movimentacao")?,
        descricao: l.texto_opc("descricao")?,
        valor: l.centavos("valor")?,
        data: l.texto_opc("data")?,
        parcela: l.inteiro_opc("parcela")?,
        total_de_parcelas: l.inteiro_opc("total_de_parcelas")?,
        fornecedor: l.id_opc("fornecedor")?,
    })
}

fn movimentacao(r: &Row) -> Result<Movimentacao, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Movimentacao {
        id: l.id("id")?,
        versao: l.versao()?,
        conta: l.id("conta")?,
        tipo: l.fechado("tipo", v::tipo_de_movimentacao)?,
        valor: l.centavos("valor")?,
        descricao: l.texto_opc("descricao")?,
        data: l.texto("data")?,
        competencia: l.texto_opc("competencia")?,
        fatura: l.id_opc("fatura")?,
        parcela_de_divida: l.id_opc("parcela_de_divida")?,
        categoria: l.id_opc("categoria")?,
        prioridade: l.fechado_opc("prioridade", v::prioridade)?,
        confirmada: l.booleano("confirmada")?,
        notas: l.texto_opc("notas")?,
        origem: l.fechado("origem", v::origem_da_movimentacao)?,
        ocorrencia: l.id_opc("ocorrencia")?,
        fornecedor: l.id_opc("fornecedor")?,
        documento: l.id_opc("documento")?,
        tags: l.ids("tags")?,
        criada_em: l.texto("criada_em")?,
    })
}

fn transferencia(r: &Row) -> Result<Transferencia, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Transferencia {
        id: l.id("id")?,
        versao: l.versao()?,
        origem: l.id("origem")?,
        destino: l.id("destino")?,
        valor: l.centavos("valor")?,
        descricao: l.texto_opc("descricao")?,
        data: l.texto("data")?,
        competencia: l.texto_opc("competencia")?,
        confirmada: l.booleano("confirmada")?,
        notas: l.texto_opc("notas")?,
    })
}

fn rateio(r: &Row) -> Result<Rateio, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Rateio {
        id: l.id("id")?,
        movimentacao: l.id("movimentacao")?,
        centro: l.id("centro")?,
        valor: l.centavos("valor")?,
        nota: l.texto_opc("nota")?,
        criado_em: l.texto("criado_em")?,
    })
}

fn categoria(r: &Row) -> Result<Categoria, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Categoria {
        id: l.id("id")?,
        versao: l.versao()?,
        nome: l.texto("nome")?,
        pai: l.id_opc("pai")?,
        icone: l.texto_opc("icone")?,
        cor: l.texto_opc("cor")?,
        ordem: l.inteiro_opc("ordem")?,
        ativa: l.booleano("ativa")?,
    })
}

fn tag(r: &Row) -> Result<Tag, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Tag {
        id: l.id("id")?,
        versao: l.versao()?,
        nome: l.texto("nome")?,
        cor: l.texto_opc("cor")?,
        influencia_prioridade: l.booleano("influencia_prioridade")?,
        prioridade_sugerida: l.fechado_opc("prioridade_sugerida", v::prioridade)?,
    })
}

fn centro(r: &Row) -> Result<Centro, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Centro {
        id: l.id("id")?,
        versao: l.versao()?,
        nome: l.texto("nome")?,
        tipo: l.fechado("tipo", v::tipo_de_centro)?,
        descricao: l.texto_opc("descricao")?,
        estado: l.fechado("estado", v::estado_do_centro)?,
        icone: l.texto_opc("icone")?,
        cor: l.texto_opc("cor")?,
    })
}

fn fornecedor(r: &Row) -> Result<Fornecedor, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Fornecedor {
        id: l.id("id")?,
        versao: l.versao()?,
        nome: l.texto("nome")?,
        nome_fantasia: l.texto_opc("nome_fantasia")?,
        razao_social: l.texto_opc("razao_social")?,
        documento: l.texto_opc("documento")?,
        tipo: l.fechado("tipo", v::tipo_de_fornecedor)?,
        site: l.texto_opc("site")?,
        notas: l.texto_opc("notas")?,
        instituicao: l.id_opc("instituicao")?,
        ativo: l.booleano("ativo")?,
        ordem: l.inteiro_opc("ordem")?,
        tags: l.ids("tags")?,
    })
}

fn apelido(r: &Row) -> Result<Apelido, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Apelido {
        id: l.id("id")?,
        versao: l.versao()?,
        fornecedor: l.id("fornecedor")?,
        nome: l.texto("nome")?,
        tipo: l.fechado("tipo", v::tipo_de_apelido)?,
        ativo: l.booleano("ativo")?,
        valido_desde: l.texto_opc("valido_desde")?,
        valido_ate: l.texto_opc("valido_ate")?,
        notas: l.texto_opc("notas")?,
    })
}

fn contrato(r: &Row) -> Result<Contrato, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Contrato {
        id: l.id("id")?,
        versao: l.versao()?,
        fornecedor: l.id("fornecedor")?,
        tipo: l.fechado("tipo", v::tipo_de_contrato)?,
        identificador: l.texto_opc("identificador")?,
        rotulo: l.texto_opc("rotulo")?,
        ativo: l.booleano("ativo")?,
        inicio: l.texto_opc("inicio")?,
        fim: l.texto_opc("fim")?,
        notas: l.texto_opc("notas")?,
    })
}

fn divida(r: &Row) -> Result<Divida, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Divida {
        id: l.id("id")?,
        versao: l.versao()?,
        conta: l.id("conta")?,
        nome: l.texto("nome")?,
        tipo: l.fechado("tipo", v::tipo_de_divida)?,
        valor_original: l.centavos("valor_original")?,
        saldo_devedor: l.centavos("saldo_devedor")?,
        taxa_de_juros: l.texto_opc("taxa_de_juros")?,
        periodo_da_taxa: l.fechado_opc("periodo_da_taxa", v::periodo_da_taxa)?,
        sistema_de_amortizacao: l
            .fechado_opc("sistema_de_amortizacao", v::sistema_de_amortizacao)?,
        total_de_parcelas: l.inteiro_opc("total_de_parcelas")?,
        parcelas_pagas: l.inteiro("parcelas_pagas")?,
        inicio: l.texto_opc("inicio")?,
        fim: l.texto_opc("fim")?,
        estado: l.fechado("estado", v::estado_da_divida)?,
        fornecedor: l.id_opc("fornecedor")?,
        tags: l.ids("tags")?,
    })
}

fn parcela(r: &Row) -> Result<Parcela, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Parcela {
        id: l.id("id")?,
        versao: l.versao()?,
        divida: l.id("divida")?,
        numero: l.inteiro("numero")?,
        vencimento: l.texto("vencimento")?,
        total: l.centavos("total")?,
        principal: l.centavos_opc("principal")?,
        juros: l.centavos_opc("juros")?,
        seguro: l.centavos_opc("seguro")?,
        tarifa: l.centavos_opc("tarifa")?,
        pago: l.centavos("pago")?,
        data_do_pagamento: l.texto_opc("data_do_pagamento")?,
        estado: l.fechado("estado", v::estado_da_parcela)?,
    })
}

fn recorrente(r: &Row) -> Result<Recorrente, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Recorrente {
        id: l.id("id")?,
        versao: l.versao()?,
        conta: l.id_opc("conta")?,
        nome: l.texto("nome")?,
        tipo: l.fechado("tipo", v::tipo_de_recorrente)?,
        valor: l.centavos_opc("valor")?,
        valor_variavel: l.booleano("valor_variavel")?,
        frequencia: l.fechado("frequencia", v::frequencia)?,
        dia_do_mes: l.inteiro_opc("dia_do_mes")?,
        intervalo_em_dias: l.inteiro_opc("intervalo_em_dias")?,
        categoria: l.id_opc("categoria")?,
        prioridade: l.fechado_opc("prioridade", v::prioridade)?,
        inicio: l.texto_opc("inicio")?,
        fim: l.texto_opc("fim")?,
        ativo: l.booleano("ativo")?,
        notas: l.texto_opc("notas")?,
        fornecedor: l.id_opc("fornecedor")?,
        tags: l.ids("tags")?,
    })
}

fn ocorrencia(r: &Row) -> Result<Ocorrencia, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Ocorrencia {
        id: l.id("id")?,
        versao: l.versao()?,
        recorrente: l.id("recorrente")?,
        data_prevista: l.texto("data_prevista")?,
        valor_previsto: l.centavos_opc("valor_previsto")?,
        valor_real: l.centavos_opc("valor_real")?,
        estado: l.fechado("estado", v::estado_da_ocorrencia)?,
        data_do_pagamento: l.texto_opc("data_do_pagamento")?,
        movimentacao: l.id_opc("movimentacao")?,
        notas: l.texto_opc("notas")?,
    })
}

fn documento(r: &Row) -> Result<Documento, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Documento {
        id: l.id("id")?,
        versao: l.versao_opc()?,
        origem: l.fechado("origem", v::origem_do_documento)?,
        nome_do_arquivo: l.texto("nome_do_arquivo")?,
        tipo_mime: l.texto_opc("tipo_mime")?,
        tamanho_em_bytes: l.valor::<Option<i64>>("tamanho_em_bytes")?,
        estado: l.texto("estado")?,
        hash_do_conteudo: l.texto_opc("hash_do_conteudo")?,
        tipo_de_documento: l.texto_opc("tipo_de_documento")?,
        fornecedor: l.id_opc("fornecedor")?,
        nome_do_fornecedor_bruto: l.texto_opc("nome_do_fornecedor_bruto")?,
        assunto_do_email: l.texto_opc("assunto_do_email")?,
        remetente_do_email: l.texto_opc("remetente_do_email")?,
        recebido_em: l.texto_opc("recebido_em")?,
        criado_em: l.texto_opc("criado_em")?,
    })
}

fn lote(r: &Row) -> Result<LoteDeRascunhos, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(LoteDeRascunhos {
        id: l.id("id")?,
        versao: l.versao()?,
        nome: l.texto_opc("nome")?,
        estado: l.fechado("estado", v::estado_do_lote)?,
        total: l.inteiro("total")?,
        aprovados: l.inteiro("aprovados")?,
        recusados: l.inteiro("recusados")?,
        documento: l.id_opc("documento")?,
        criado_em: l.texto_opc("criado_em")?,
    })
}

fn rascunho(r: &Row) -> Result<Rascunho, ErroDeLeitura> {
    let l = Leitor(r);
    let tipo_texto = l.texto("tipo")?;
    let versao_do_esquema = i16::try_from(l.inteiro("versao_do_esquema")?).unwrap_or(0);
    let dados: serde_json::Value = l.valor("dados")?;
    Ok(Rascunho {
        id: l.id("id")?,
        versao: l.versao()?,
        lote: l.id_opc("lote")?,
        documento: l.id_opc("documento")?,
        tipo: l.fechado("tipo", v::tipo_de_rascunho)?,
        estado: l.fechado("estado", v::estado_do_rascunho)?,
        dados: crate::rascunhos::dados_v1(&tipo_texto, &dados, versao_do_esquema),
        versao_do_esquema,
        correcoes: l.valor::<Option<serde_json::Value>>("correcoes")?,
        confianca_percentual: l.inteiro_opc("confianca")?,
        conciliacao: l.fechado("conciliacao", v::estado_da_conciliacao)?,
        movimentacao_conciliada: l.id_opc("movimentacao_conciliada")?,
        lancado_como: l.id_opc("lancado_como")?,
        criado_em: l.texto_opc("criado_em")?,
    })
}
