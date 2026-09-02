/**
 * Vocabulário de categorias da Pluggy → português.
 *
 * A Pluggy devolve `category` em inglês ("Groceries", "Transfer - PIX"). Sem
 * esta tabela o usuário brasileiro vê a própria vida financeira rotulada num
 * idioma que não é o dele — e, pior, com nomes que não batem com as
 * categorias que ele já criou à mão.
 *
 * Duas decisões deliberadas:
 *
 *   - **Sugestões diferentes podem convergir no mesmo nome.** "Gyms" e
 *     "Gyms and fitness centers" viram ambas "Academia"; "Transfer - TED",
 *     "Transfer - DOC" e "Transfer - Check" viram "Transferências". Isso é o
 *     ponto: o usuário quer UMA linha no relatório, não três.
 *   - **Sem tradução, devolvemos o nome como veio.** Inventar tradução para
 *     um código desconhecido é pior do que mostrar o código: o nome original
 *     é rastreável até a documentação da Pluggy, um chute não é.
 */

const TRADUCAO: Record<string, string> = {
  // Transferências e PIX
  "Transfer - PIX": "PIX",
  "Same person transfer": "Transferência entre contas próprias",
  "Same person transfer - CASH": "Transferência entre contas próprias",
  "Transfer - Internal": "Transferência entre contas próprias",
  Transfers: "Transferências",
  "Transfer - TED": "Transferências",
  "Transfer - DOC": "Transferências",
  "Transfer - Check": "Transferências",
  "Transfer - Bank Slip": "Pagamento de boleto",
  "Transfer - Foreign Exchange": "Câmbio",
  Withdrawal: "Saque",
  "Withdrawal by check": "Saque",
  Deposit: "Depósito",

  // Casa e consumo
  Groceries: "Mercado",
  Supermarket: "Mercado",
  "Eating out": "Restaurantes",
  "Food and drinks": "Alimentação",
  "Food delivery": "Delivery",
  Housing: "Moradia",
  Rent: "Aluguel",
  Houseware: "Utensílios domésticos",
  Utilities: "Contas de consumo",
  Electricity: "Energia elétrica",
  Water: "Água",
  Internet: "Internet",
  Telecommunications: "Telefonia e internet",
  "Mobile phone": "Celular",
  "Bill payment": "Pagamento de contas",
  Boleto: "Pagamento de contas",

  // Transporte
  "Taxi and ride-hailing": "Transporte por app",
  Transportation: "Transporte",
  "Public transportation": "Transporte público",
  "Gas stations": "Combustível",
  Parking: "Estacionamento",
  Tolls: "Pedágio",
  Automotive: "Automotivo",
  "Car maintenance": "Manutenção do carro",
  "Vehicle maintenance": "Manutenção do veículo",

  // Saúde e bem-estar
  Health: "Saúde",
  Healthcare: "Saúde",
  Pharmacy: "Farmácia",
  Gyms: "Academia",
  "Gyms and fitness centers": "Academia",
  "Wellness and fitness": "Bem-estar e fitness",

  // Educação e cultura
  Education: "Educação",
  "Online education": "Educação",
  "Online Courses": "Educação",
  University: "Educação",
  School: "Educação",
  Bookstore: "Livraria",
  "Cinema, theater and concerts": "Cinema, teatro e shows",
  Tickets: "Ingressos",
  Entertainment: "Lazer",
  Leisure: "Lazer",

  // Compras
  Shopping: "Compras",
  "Online shopping": "Compras online",
  Clothing: "Vestuário",
  Electronics: "Eletrônicos",
  "Sports goods": "Artigos esportivos",
  "Kids and toys": "Filhos e brinquedos",
  "Office supplies": "Material de escritório",
  "Veterinary and pet shops": "Pet",
  Pets: "Pet",

  // Serviços
  Services: "Serviços",
  "Digital services": "Serviços digitais",
  "Video streaming": "Streaming",
  "Music streaming": "Streaming",
  Subscriptions: "Assinaturas",

  // Viagem
  Travel: "Viagens",
  Airfare: "Passagens aéreas",
  "Airport and airlines": "Passagens aéreas",
  Lodging: "Hospedagem",
  Accomodation: "Hospedagem",
  "Mileage programs": "Programa de milhas",

  // Dinheiro: crédito, juros, impostos
  Loans: "Empréstimos e financiamentos",
  "Loans and financing": "Empréstimos e financiamentos",
  "Real estate financing": "Financiamento imobiliário",
  "Credit card payment": "Pagamento de fatura",
  "Credit card fees": "Tarifas de cartão",
  "Interests charged": "Juros cobrados",
  "Late payment fees": "Multas e juros por atraso",
  "Late payment and overdraft costs": "Multas e juros por atraso",
  "Bank fees": "Tarifas bancárias",
  "Account fees": "Tarifas bancárias",
  "Wire transfer fees": "Tarifas bancárias",
  "Tax on financial operations": "IOF",
  Taxes: "Impostos",
  "Income taxes": "Imposto de renda",
  "Government fees": "Taxas públicas",

  // Entradas e investimentos
  Salary: "Salário",
  "Non-recurring income": "Receita eventual",
  "Proceeds interests and dividends": "Rendimentos",
  Investments: "Investimentos",
  "Investment application": "Aplicação em investimento",
  "Investment redemption": "Resgate de investimento",
  Pension: "Previdência",
  Cashback: "Cashback",
  Refunds: "Reembolsos",

  // Outros
  Insurance: "Seguros",
  Donations: "Doações",
  Lottery: "Loteria",
  Gambling: "Loteria",
  "Gambling and lottery": "Loteria",
};

/**
 * Nome em português da categoria sugerida pela Pluggy.
 * `null` quando não há sugestão — a transação fica sem categoria, e sem
 * categoria é mais honesto do que numa categoria inventada.
 */
export function nomeDeCategoria(sugestao: string | null | undefined): string | null {
  const bruta = sugestao?.trim();
  if (!bruta) return null;
  return TRADUCAO[bruta] ?? bruta;
}
