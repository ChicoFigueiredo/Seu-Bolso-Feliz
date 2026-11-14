//! O vocabulário de categorias da Pluggy em português (`categorias.ts`).
//!
//! A Pluggy devolve `category` em inglês ("Groceries", "Transfer - PIX"). Sugestões diferentes
//! podem convergir no mesmo nome (o usuário quer UMA linha no relatório). Sem tradução, o
//! nome volta como veio: inventar tradução para um código desconhecido é pior do que mostrar
//! o código.

const TRADUCAO: [(&str, &str); 103] = [
    ("Transfer - PIX", "PIX"),
    (
        "Same person transfer",
        "Transferência entre contas próprias",
    ),
    (
        "Same person transfer - CASH",
        "Transferência entre contas próprias",
    ),
    ("Transfer - Internal", "Transferência entre contas próprias"),
    ("Transfers", "Transferências"),
    ("Transfer - TED", "Transferências"),
    ("Transfer - DOC", "Transferências"),
    ("Transfer - Check", "Transferências"),
    ("Transfer - Bank Slip", "Pagamento de boleto"),
    ("Transfer - Foreign Exchange", "Câmbio"),
    ("Withdrawal", "Saque"),
    ("Withdrawal by check", "Saque"),
    ("Deposit", "Depósito"),
    ("Groceries", "Mercado"),
    ("Supermarket", "Mercado"),
    ("Eating out", "Restaurantes"),
    ("Food and drinks", "Alimentação"),
    ("Food delivery", "Delivery"),
    ("Housing", "Moradia"),
    ("Rent", "Aluguel"),
    ("Houseware", "Utensílios domésticos"),
    ("Utilities", "Contas de consumo"),
    ("Electricity", "Energia elétrica"),
    ("Water", "Água"),
    ("Internet", "Internet"),
    ("Telecommunications", "Telefonia e internet"),
    ("Mobile phone", "Celular"),
    ("Bill payment", "Pagamento de contas"),
    ("Boleto", "Pagamento de contas"),
    ("Taxi and ride-hailing", "Transporte por app"),
    ("Transportation", "Transporte"),
    ("Public transportation", "Transporte público"),
    ("Gas stations", "Combustível"),
    ("Parking", "Estacionamento"),
    ("Tolls", "Pedágio"),
    ("Automotive", "Automotivo"),
    ("Car maintenance", "Manutenção do carro"),
    ("Vehicle maintenance", "Manutenção do veículo"),
    ("Health", "Saúde"),
    ("Healthcare", "Saúde"),
    ("Pharmacy", "Farmácia"),
    ("Gyms", "Academia"),
    ("Gyms and fitness centers", "Academia"),
    ("Wellness and fitness", "Bem-estar e fitness"),
    ("Education", "Educação"),
    ("Online education", "Educação"),
    ("Online Courses", "Educação"),
    ("University", "Educação"),
    ("School", "Educação"),
    ("Bookstore", "Livraria"),
    ("Cinema, theater and concerts", "Cinema, teatro e shows"),
    ("Tickets", "Ingressos"),
    ("Entertainment", "Lazer"),
    ("Leisure", "Lazer"),
    ("Shopping", "Compras"),
    ("Online shopping", "Compras online"),
    ("Clothing", "Vestuário"),
    ("Electronics", "Eletrônicos"),
    ("Sports goods", "Artigos esportivos"),
    ("Kids and toys", "Filhos e brinquedos"),
    ("Office supplies", "Material de escritório"),
    ("Veterinary and pet shops", "Pet"),
    ("Pets", "Pet"),
    ("Services", "Serviços"),
    ("Digital services", "Serviços digitais"),
    ("Video streaming", "Streaming"),
    ("Music streaming", "Streaming"),
    ("Subscriptions", "Assinaturas"),
    ("Travel", "Viagens"),
    ("Airfare", "Passagens aéreas"),
    ("Airport and airlines", "Passagens aéreas"),
    ("Lodging", "Hospedagem"),
    ("Accomodation", "Hospedagem"),
    ("Mileage programs", "Programa de milhas"),
    ("Loans", "Empréstimos e financiamentos"),
    ("Loans and financing", "Empréstimos e financiamentos"),
    ("Real estate financing", "Financiamento imobiliário"),
    ("Credit card payment", "Pagamento de fatura"),
    ("Credit card fees", "Tarifas de cartão"),
    ("Interests charged", "Juros cobrados"),
    ("Late payment fees", "Multas e juros por atraso"),
    (
        "Late payment and overdraft costs",
        "Multas e juros por atraso",
    ),
    ("Bank fees", "Tarifas bancárias"),
    ("Account fees", "Tarifas bancárias"),
    ("Wire transfer fees", "Tarifas bancárias"),
    ("Tax on financial operations", "IOF"),
    ("Taxes", "Impostos"),
    ("Income taxes", "Imposto de renda"),
    ("Government fees", "Taxas públicas"),
    ("Salary", "Salário"),
    ("Non-recurring income", "Receita eventual"),
    ("Proceeds interests and dividends", "Rendimentos"),
    ("Investments", "Investimentos"),
    ("Investment application", "Aplicação em investimento"),
    ("Investment redemption", "Resgate de investimento"),
    ("Pension", "Previdência"),
    ("Cashback", "Cashback"),
    ("Refunds", "Reembolsos"),
    ("Insurance", "Seguros"),
    ("Donations", "Doações"),
    ("Lottery", "Loteria"),
    ("Gambling", "Loteria"),
    ("Gambling and lottery", "Loteria"),
];

/// O nome em português da categoria sugerida pela Pluggy. `None` quando não há sugestão: sem
/// categoria é mais honesto do que numa categoria inventada.
pub fn nome_de_categoria(sugestao: Option<&str>) -> Option<String> {
    let bruta = sugestao?.trim();
    if bruta.is_empty() {
        return None;
    }
    Some(
        TRADUCAO
            .iter()
            .find(|(en, _)| *en == bruta)
            .map_or(bruta, |(_, pt)| pt)
            .to_owned(),
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn traduz_converge_e_devolve_o_desconhecido() {
        assert_eq!(
            nome_de_categoria(Some("Groceries")).as_deref(),
            Some("Mercado")
        );
        assert_eq!(
            nome_de_categoria(Some("  Transfer - PIX ")).as_deref(),
            Some("PIX")
        );
        assert_eq!(
            nome_de_categoria(Some("Gyms")),
            nome_de_categoria(Some("Gyms and fitness centers"))
        );
        assert_eq!(
            nome_de_categoria(Some("Transfer - TED")),
            nome_de_categoria(Some("Transfer - Check"))
        );
        assert_eq!(
            nome_de_categoria(Some("Coisa Nova")).as_deref(),
            Some("Coisa Nova")
        );
        assert_eq!(nome_de_categoria(Some("  ")), None);
        assert_eq!(nome_de_categoria(None), None);
    }

    #[test]
    fn nao_ha_chave_repetida() {
        let mut chaves: Vec<_> = TRADUCAO.iter().map(|(k, _)| *k).collect();
        chaves.sort_unstable();
        let total = chaves.len();
        chaves.dedup();
        assert_eq!(chaves.len(), total);
    }
}
