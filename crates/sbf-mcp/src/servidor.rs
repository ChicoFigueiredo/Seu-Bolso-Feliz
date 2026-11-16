//! A casca do `rmcp`: nomes, descrições e esquemas das ferramentas. A lógica está em
//! [`crate::ferramentas`]. No stderr vai só o nome da ferramenta e o tamanho da resposta,
//! nunca o conteúdo nem os argumentos.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{ServiceExt, schemars, tool, tool_handler, tool_router, transport::stdio};
use serde::Deserialize;
use serde_json::Value;

use crate::ferramentas::{
    Nucleo, ParamsDeAchados, ParamsDeDecisoes, ParamsDeFornecedores, ParamsDeRegistros,
    ParamsDeSugestoesAbertas, ParamsDoLivro, SugestaoProposta,
};
use crate::plano::{
    ComChave, EntradaDeAlteracao as Alt, FERRAMENTAS as FERRAMENTAS_DO_PLANO, PedidoDeAlteracao,
    PedidoDeCancelamento, PedidoDeCompromisso, PedidoDeConciliacao, PedidoDeExcecao,
    PedidoDeExclusao, PedidoDeLista, PedidoDePausa, PedidoDePeriodo, PedidoDeProjecao,
    PedidoDePropostas, PedidoDeRetomada, PedidoDeSimulacao, PedidoNovoCompromisso,
};

macro_rules! esquema {
    ($nome:ident, $alvo:ident { $( $(#[$m:meta])* $campo:ident : $tipo:ty ),* $(,)? }) => {
        #[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
        pub struct $nome { $( $(#[$m])* pub $campo: $tipo, )* }
        impl From<$nome> for $alvo {
            fn from(p: $nome) -> Self { Self { $( $campo: p.$campo ),* } }
        }
    };
}

esquema!(PedidoDeRegistros, ParamsDeRegistros {
    /// Trecho da descrição ou do fornecedor (sem diferenciar maiúsculas).
    texto: Option<String>,
    /// Trecho do nome do fornecedor.
    fornecedor: Option<String>,
    /// Categoria exata (sem diferenciar maiúsculas).
    categoria: Option<String>,
    /// Data inicial, AAAA-MM-DD, inclusive.
    de: Option<String>,
    /// Data final, AAAA-MM-DD, inclusive.
    ate: Option<String>,
    /// extraido, conferido, divergente, aprovado ou promovido.
    estado: Option<String>,
    /// true: só os registros sem categoria.
    sem_categoria: Option<bool>,
    /// Itens por página (1 a 100, padrão 25).
    limite: Option<i64>,
    /// Quantos itens pular (use proximo_deslocamento da resposta anterior).
    deslocamento: Option<i64>,
});

esquema!(PedidoDeAchados, ParamsDeAchados {
    /// novo, documento, descartado ou duvida.
    estado: Option<String>,
    /// Tipo de documento (boleto, fatura, extrato, comprovante, contracheque...).
    tipo: Option<String>,
    /// Trecho do nome do arquivo, do assunto do e-mail ou do fornecedor.
    texto: Option<String>,
    /// Itens por página (1 a 100, padrão 25).
    limite: Option<i64>,
    /// Quantos itens pular.
    deslocamento: Option<i64>,
});

esquema!(PedidoDeFornecedores, ParamsDeFornecedores {
    /// Trecho do nome.
    texto: Option<String>,
    /// Itens por página (1 a 100, padrão 25).
    limite: Option<i64>,
    /// Quantos itens pular.
    deslocamento: Option<i64>,
});

esquema!(PedidoDeDecisoes, ParamsDeDecisoes {
    /// achado, registro, fornecedor, lote ou pendencia.
    alvo_tipo: Option<String>,
    /// Itens por página (1 a 100, padrão 25).
    limite: Option<i64>,
    /// Quantos itens pular.
    deslocamento: Option<i64>,
});

esquema!(PedidoDoLivro, ParamsDoLivro {
    /// Trecho da descrição.
    texto: Option<String>,
    /// Nome da categoria.
    categoria: Option<String>,
    /// Nome da conta.
    conta: Option<String>,
    /// Data inicial, AAAA-MM-DD, inclusive.
    de: Option<String>,
    /// Data final, AAAA-MM-DD, inclusive.
    ate: Option<String>,
    /// true: só as movimentações sem categoria.
    sem_categoria: Option<bool>,
    /// Itens por página (1 a 100, padrão 25).
    limite: Option<i64>,
    /// Quantos itens pular.
    deslocamento: Option<i64>,
});

esquema!(PedidoDeSugestoesAbertas, ParamsDeSugestoesAbertas {
    /// achado ou registro.
    alvo_tipo: Option<String>,
    /// Campo sugerido (categoria, fornecedor, e_financeiro...).
    campo: Option<String>,
    /// Quem sugeriu (por exemplo "mcp").
    modelo: Option<String>,
    /// Só com confiança a partir deste valor (0 a 1).
    confianca_minima: Option<f64>,
    /// Itens por página (1 a 100, padrão 25).
    limite: Option<i64>,
    /// Quantos itens pular.
    deslocamento: Option<i64>,
});

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeSemelhantes {
    /// O texto a procurar (uma descrição, um fornecedor, uma frase).
    pub texto: String,
    /// Quantos trechos (1 a 20, padrão 5).
    pub limite: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeRegistro {
    /// O id numérico do registro.
    pub id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeTexto {
    /// O content_hash do achado (veja buscar_achados).
    pub content_hash: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ItemSugerido {
    /// "achado" ou "registro".
    pub alvo_tipo: String,
    /// O content_hash do achado, ou o id do registro.
    pub alvo_id: String,
    /// Achado: e_financeiro (sim|nao|incerto), tipo_documento, fornecedor, data_chave
    /// (AAAA-MM-DD), valor (centavos inteiros). Registro: categoria, fornecedor, conta,
    /// tipo, duplicata_de (id de um registro parecido).
    pub campo: String,
    /// O valor sugerido, no formato do campo.
    pub valor: Value,
    /// De 0 a 1.
    pub confianca: f64,
    /// Por que, em até 140 caracteres.
    pub justificativa: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeSugestoes {
    /// As sugestões (até 100 por chamada).
    pub sugestoes: Vec<ItemSugerido>,
}

/// O servidor. Barato de clonar.
#[derive(Debug, Clone)]
pub struct Servidor {
    nucleo: Nucleo,
    tool_router: ToolRouter<Servidor>,
}

impl Servidor {
    /// Sem planejamento ligado, as ferramentas do controle mensal nem aparecem na lista.
    pub fn novo(nucleo: Nucleo) -> Self {
        let mut tool_router = Self::tool_router();
        if !nucleo.tem_planejamento() {
            for nome in FERRAMENTAS_DO_PLANO {
                tool_router.remove_route(nome);
            }
        }
        Self {
            nucleo,
            tool_router,
        }
    }

    /// Os nomes das ferramentas que este servidor anuncia.
    pub fn nomes_das_ferramentas(&self) -> Vec<String> {
        self.tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }
}

/// Registra no stderr só o nome e o tamanho (nunca o conteúdo) e monta a resposta.
fn responder(nome: &str, r: Result<Value, String>) -> Result<String, String> {
    let r = r.map(|v| v.to_string());
    let tamanho = match &r {
        Ok(t) | Err(t) => t.len(),
    };
    eprintln!(
        "sbf mcp: {nome} -> {tamanho} bytes{}",
        if r.is_err() { " (erro)" } else { "" }
    );
    r
}

#[tool_router]
impl Servidor {
    #[tool(
        description = "Totais do inventario: achados e registros por estado, quantos sem categoria, quantos o usuario ja decidiu, sugestoes abertas por campo e se ha livro-caixa. Comece por aqui."
    )]
    fn inventario_resumo(&self) -> Result<String, String> {
        responder("inventario_resumo", self.nucleo.inventario_resumo())
    }

    #[tool(
        description = "Busca registros (fatos financeiros extraidos de documentos) com filtros, do mais novo ao mais antigo. Valores em centavos (negativo = saiu). Paginado."
    )]
    fn buscar_registros(
        &self,
        Parameters(p): Parameters<PedidoDeRegistros>,
    ) -> Result<String, String> {
        responder("buscar_registros", self.nucleo.buscar_registros(&p.into()))
    }

    #[tool(
        description = "Um registro completo: campos, sub-registros, achado de origem, sugestoes abertas e decisoes do usuario."
    )]
    fn ver_registro(&self, Parameters(p): Parameters<PedidoDeRegistro>) -> Result<String, String> {
        responder("ver_registro", self.nucleo.ver_registro(p.id))
    }

    #[tool(
        description = "Busca achados (documentos unicos, pelo content_hash) por estado, tipo e texto. Use ver_texto_do_achado para ler o conteudo."
    )]
    fn buscar_achados(&self, Parameters(p): Parameters<PedidoDeAchados>) -> Result<String, String> {
        responder("buscar_achados", self.nucleo.buscar_achados(&p.into()))
    }

    #[tool(
        description = "O texto extraido de um achado, como a triagem o ve: ate 3000 caracteres do comeco e 1000 do fim. Os numeros longos (CPF, conta, cartao) vem mascarados, no nivel de confianca deste cliente."
    )]
    fn ver_texto_do_achado(
        &self,
        Parameters(p): Parameters<PedidoDeTexto>,
    ) -> Result<String, String> {
        responder(
            "ver_texto_do_achado",
            self.nucleo.ver_texto_do_achado(&p.content_hash),
        )
    }

    #[tool(
        description = "Fornecedores dos registros, do mais frequente ao menos, com a contagem, quantos estao sem categoria e a categoria que o usuario ja fixou por tipo."
    )]
    fn fornecedores(
        &self,
        Parameters(p): Parameters<PedidoDeFornecedores>,
    ) -> Result<String, String> {
        responder("fornecedores", self.nucleo.fornecedores(&p.into()))
    }

    #[tool(
        description = "As decisoes mais recentes do usuario no log (antes e depois), uteis para imitar o criterio dele."
    )]
    fn decisoes_recentes(
        &self,
        Parameters(p): Parameters<PedidoDeDecisoes>,
    ) -> Result<String, String> {
        responder(
            "decisoes_recentes",
            self.nucleo.decisoes_recentes(&p.into()),
        )
    }

    #[tool(
        description = "Movimentacoes do livro-caixa (copia local, so leitura) com categoria e conta por nome. Erro se a estacao nao tem copia."
    )]
    fn livro_movimentacoes(
        &self,
        Parameters(p): Parameters<PedidoDoLivro>,
    ) -> Result<String, String> {
        responder(
            "livro_movimentacoes",
            self.nucleo.livro_movimentacoes(&p.into()),
        )
    }

    #[tool(
        description = "PROPOE sugestoes (nao decide nada). Cada item: alvo_tipo, alvo_id, campo, valor, confianca 0-1, justificativa ate 140 caracteres. Recusa alvo inexistente, ja decidido pelo usuario ou promovido, e campo ou valor invalido, dizendo o motivo de cada um. O usuario confirma em Inventario > Triagem por IA."
    )]
    fn sugerir(&self, Parameters(p): Parameters<PedidoDeSugestoes>) -> Result<String, String> {
        let lista: Vec<SugestaoProposta> = p
            .sugestoes
            .into_iter()
            .map(|s| SugestaoProposta {
                alvo_tipo: s.alvo_tipo,
                alvo_id: s.alvo_id,
                campo: s.campo,
                valor: s.valor,
                confianca: s.confianca,
                justificativa: s.justificativa,
            })
            .collect();
        responder("sugerir", self.nucleo.sugerir(&lista))
    }

    #[tool(
        description = "Busca por semelhanca de texto no indice local: trechos de achados, registros e movimentacoes parecidos com o texto dado, com tipo, id e nota. Os numeros longos (CPF, conta, cartao) vem mascarados. Para o item inteiro use ver_registro; o texto de um achado so se o cliente tem esse direito. Erro se o indice ainda nao foi montado."
    )]
    fn buscar_semelhantes(
        &self,
        Parameters(p): Parameters<PedidoDeSemelhantes>,
    ) -> Result<String, String> {
        responder(
            "buscar_semelhantes",
            self.nucleo.buscar_semelhantes(&p.texto, p.limite),
        )
    }

    #[tool(
        description = "Sugestoes ainda abertas (aguardando o usuario), das regras, da IA e do MCP, mais novas primeiro."
    )]
    fn sugestoes_abertas(
        &self,
        Parameters(p): Parameters<PedidoDeSugestoesAbertas>,
    ) -> Result<String, String> {
        responder(
            "sugestoes_abertas",
            self.nucleo.sugestoes_abertas(&p.into()),
        )
    }

    #[tool(
        description = "Lista os compromissos (ocorrências do controle mensal) de um ciclo ou mês, na ordem de prioridade, com filtro, busca e paginação (por_pagina até 100). Valores em centavos. Só leitura."
    )]
    fn listar_compromissos(
        &self,
        Parameters(p): Parameters<PedidoDeLista>,
    ) -> Result<String, String> {
        self.plano("listar_compromissos", |l| l.listar_compromissos(&p))
    }

    #[tool(
        description = "Resumo de caixa do ciclo ou mês: entradas e saídas previstas, descontos em folha, em atraso, passagem pendente, livre após compromissos e o que falta para o número ficar certo. Só leitura."
    )]
    fn resumo_de_caixa(
        &self,
        Parameters(p): Parameters<PedidoDePeriodo>,
    ) -> Result<String, String> {
        self.plano("resumo_de_caixa", |l| l.resumo_de_caixa(&p))
    }

    #[tool(
        description = "Projeta os totais mês a mês dos compromissos, a partir de AAAA-MM, por até 360 meses (30 anos). Só leitura."
    )]
    fn projetar_compromissos(
        &self,
        Parameters(p): Parameters<PedidoDeProjecao>,
    ) -> Result<String, String> {
        self.plano("projetar_compromissos", |l| l.projetar_compromissos(&p))
    }

    #[tool(
        description = "Um compromisso: a regra, as versões, pausas, próximas ocorrências e o histórico curto. Aceita o id de uma ocorrência (rec:AAAA-MM ou rec:AAAA-MM:pN) ou o id do recorrente. Só leitura."
    )]
    fn obter_compromisso(
        &self,
        Parameters(p): Parameters<PedidoDeCompromisso>,
    ) -> Result<String, String> {
        self.plano("obter_compromisso", |l| l.obter_compromisso(&p))
    }

    #[tool(
        description = "Simula uma mudança (novo compromisso, alteração, pausa, retomada, cancelamento, exclusão lógica, conciliação ou exceção) e mostra os meses afetados, antes e depois. Não grava nada, nem proposta."
    )]
    fn simular_mudanca(
        &self,
        Parameters(p): Parameters<PedidoDeSimulacao>,
    ) -> Result<String, String> {
        self.plano("simular_mudanca", |l| l.simular_mudanca(&p))
    }

    #[tool(
        description = "Lista as propostas feitas pelo MCP e o estado de cada uma (pendente, confirmada, recusada, expirada). Só leitura; confirmar e recusar acontecem na janela do SBF."
    )]
    fn listar_propostas(
        &self,
        Parameters(p): Parameters<PedidoDePropostas>,
    ) -> Result<String, String> {
        self.plano("listar_propostas", |l| l.listar_propostas(&p))
    }

    #[tool(
        description = "PROPÕE um compromisso novo. Exige chave de idempotência. Nada muda até o usuário confirmar na janela do SBF; a resposta traz a simulação (meses afetados, antes e depois)."
    )]
    fn propor_compromisso(
        &self,
        Parameters(p): Parameters<ComChave<PedidoNovoCompromisso>>,
    ) -> Result<String, String> {
        self.propor("propor_compromisso", &p.chave, Alt::Compromisso(p.pedido))
    }

    #[tool(
        description = "PROPÕE alterar um compromisso (valor, certeza, conta, centro...), só neste mês (so_este_mes, apenas o valor) ou daqui em diante (daqui_em_diante). Exige chave de idempotência. Nada muda até o usuário confirmar na janela do SBF."
    )]
    fn propor_alteracao(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeAlteracao>>,
    ) -> Result<String, String> {
        self.propor("propor_alteracao", &p.chave, Alt::Alteracao(p.pedido))
    }

    #[tool(
        description = "PROPÕE pausar um compromisso por um período (ate é exclusivo; sem ate, até retomar). Exige chave de idempotência. Nada muda até o usuário confirmar na janela do SBF."
    )]
    fn propor_pausa(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDePausa>>,
    ) -> Result<String, String> {
        self.propor("propor_pausa", &p.chave, Alt::Pausa(p.pedido))
    }

    #[tool(
        description = "PROPÕE retomar um compromisso pausado a partir de um mês. Exige chave de idempotência. Nada muda até o usuário confirmar na janela do SBF."
    )]
    fn propor_retomada(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeRetomada>>,
    ) -> Result<String, String> {
        self.propor("propor_retomada", &p.chave, Alt::Retomada(p.pedido))
    }

    #[tool(
        description = "PROPÕE marcar um compromisso como cancelado a partir de um mês (só no app; não cancela nada no fornecedor). Exige chave de idempotência. Nada muda até o usuário confirmar na janela do SBF."
    )]
    fn propor_cancelamento(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeCancelamento>>,
    ) -> Result<String, String> {
        self.propor("propor_cancelamento", &p.chave, Alt::Cancelamento(p.pedido))
    }

    #[tool(
        description = "PROPÕE excluir (logicamente) um compromisso, com motivo; o histórico fica. Exige chave de idempotência. Nada muda até o usuário confirmar na janela do SBF."
    )]
    fn propor_exclusao_logica(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeExclusao>>,
    ) -> Result<String, String> {
        self.propor(
            "propor_exclusao_logica",
            &p.chave,
            Alt::ExclusaoLogica(p.pedido),
        )
    }

    #[tool(
        description = "PROPÕE ligar uma ocorrência a um movimento real (como sugestão de conciliação). Exige chave de idempotência. Nada muda até o usuário confirmar na janela do SBF."
    )]
    fn propor_conciliacao(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeConciliacao>>,
    ) -> Result<String, String> {
        self.propor("propor_conciliacao", &p.chave, Alt::Conciliacao(p.pedido))
    }

    #[tool(
        description = "PROPÕE uma exceção em uma ocorrência: adiar, dispensar (com motivo), pago_informado ou valor_so_deste_mes. Exige chave de idempotência. Nada muda até o usuário confirmar na janela do SBF."
    )]
    fn propor_excecao(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeExcecao>>,
    ) -> Result<String, String> {
        self.propor("propor_excecao", &p.chave, Alt::Excecao(p.pedido))
    }
}

impl Servidor {
    fn plano(
        &self,
        nome: &str,
        f: impl FnOnce(&crate::plano::Plano) -> Result<Value, String>,
    ) -> Result<String, String> {
        responder(nome, self.nucleo.plano().and_then(f))
    }

    fn propor(&self, nome: &str, chave: &str, e: Alt) -> Result<String, String> {
        responder(
            nome,
            self.nucleo.plano().and_then(|l| l.propor(nome, chave, &e)),
        )
    }
}

#[tool_handler(
    router = self.tool_router,
    name = "sbf",
    version = "0.1.0",
    instructions = "Inventario financeiro e livro-caixa do usuario, somente leitura, mais a ferramenta sugerir, que apenas PROPOE classificacoes: o usuario confirma depois no app (Inventario > Triagem por IA). Valores em centavos. Comece por inventario_resumo; leia fornecedores e decisoes_recentes para seguir o criterio do usuario antes de sugerir."
)]
impl rmcp::ServerHandler for Servidor {}

/// Serve o MCP por stdio até o cliente fechar a conexão.
///
/// # Errors
/// Falha ao iniciar o serviço ou no transporte.
pub async fn servir_stdio(nucleo: Nucleo) -> Result<(), String> {
    let servico = Servidor::novo(nucleo)
        .serve(stdio())
        .await
        .map_err(|e| e.to_string())?;
    servico.waiting().await.map_err(|e| e.to_string())?;
    Ok(())
}
