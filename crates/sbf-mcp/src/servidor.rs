//! A casca do `rmcp`: nomes, descrições e esquemas das ferramentas. A lógica está em
//! [`crate::ferramentas`]. No stderr vai só o nome da ferramenta e o tamanho da resposta,
//! nunca o conteúdo nem os argumentos.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{ServiceExt, schemars, tool, tool_handler, tool_router, transport::stdio};
use serde::Deserialize;
use serde_json::Value;

use crate::ferramentas::{
    Nucleo, ParamsDeAchados, ParamsDeColetaDoGmail, ParamsDeDecisoes, ParamsDeFornecedores,
    ParamsDeRegistros, ParamsDeSugestoesAbertas, ParamsDoLivro, SugestaoProposta,
};
use crate::plano::{
    ComChave, EntradaDeAlteracao as Alt, FERRAMENTAS as FERRAMENTAS_DO_PLANO,
    FERRAMENTAS_DE_ESCRITA as ESCRITA_DO_PLANO, PedidoDeAjustarQuantidade, PedidoDeAlteracao,
    PedidoDeAplicacao, PedidoDeAplicarModelo, PedidoDeAplicarProposta, PedidoDeCancelamento,
    PedidoDeCompromisso, PedidoDeConciliacao, PedidoDeConciliar, PedidoDeCriarModelo,
    PedidoDeDefinirParametro, PedidoDeDesconciliar, PedidoDeDesfazer, PedidoDeExcecao,
    PedidoDeExclusao, PedidoDeLista, PedidoDePausa, PedidoDePeriodo, PedidoDeProjecao,
    PedidoDePropostas, PedidoDeRecusaDeProposta, PedidoDeRetomada, PedidoDeSimulacao,
    PedidoNovoCompromisso,
};
use crate::regras::{
    FERRAMENTAS as FERRAMENTAS_DAS_REGRAS, FERRAMENTAS_DE_ESCRITA as ESCRITA_DAS_REGRAS,
    PedidoDeProposta, PedidoDeRegra, PedidoDeSimulacao as PedidoDeSimulacaoDeRegra, PedidoDeTeste,
    PedidoDeVersao, Regras,
};
use crate::roteiros_escritos::{
    FERRAMENTAS as FERRAMENTAS_DOS_ROTEIROS_ESCRITOS, PedidoDeRoteiroEscrito,
    PedidoDeVerRoteiroEscrito, RoteirosEscritos,
};
use crate::telegram::{
    FERRAMENTAS as FERRAMENTAS_DO_TELEGRAM, PedidoDeColeta, PedidoDeConversas, PedidoDeSeguir,
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
    /// A conta: o rótulo (CX-0001 · Apelido), o código, o apelido ou o nome do banco.
    conta: Option<String>,
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

esquema!(PedidoDeColetaDoGmail, ParamsDeColetaDoGmail {
    /// Consulta do Gmail (a mesma da caixa de busca). Padrão: has:attachment newer_than:30d -category:promotions.
    consulta: Option<String>,
    /// AAAA-MM-DD: coleta desde esta data e ignora o checkpoint.
    desde: Option<String>,
    /// Quantas mensagens, no máximo, nesta coleta (padrão 200).
    limite: Option<usize>,
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
    /// Nome da conta ou o rótulo dela (código, apelido).
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
            for nome in FERRAMENTAS_DO_PLANO.iter().chain(&ESCRITA_DO_PLANO) {
                tool_router.remove_route(nome);
            }
        }
        if !nucleo.tem_gmail() {
            tool_router.remove_route("gmail_coletar");
        }
        if !nucleo.tem_regras() {
            for nome in FERRAMENTAS_DAS_REGRAS.iter().chain(&ESCRITA_DAS_REGRAS) {
                tool_router.remove_route(nome);
            }
        }
        if !nucleo.tem_roteiros_escritos() {
            for nome in &FERRAMENTAS_DOS_ROTEIROS_ESCRITOS {
                tool_router.remove_route(nome);
            }
        }
        if !nucleo.tem_telegram() {
            for nome in &FERRAMENTAS_DO_TELEGRAM {
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
        description = "Busca registros (fatos financeiros extraidos de documentos) com filtros, do mais novo ao mais antigo. Valores em centavos (negativo = saiu). Cada registro traz conta (a do extrato) e conta_rotulo (codigo e apelido que o usuario deu, como CX-0001 · Reserva); o filtro conta aceita um ou outro. Paginado."
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
        description = "PROPOE sugestoes (nao decide nada). Cada item: alvo_tipo, alvo_id, campo, valor, confianca 0-1, justificativa ate 140 caracteres. Recusa alvo inexistente, ja decidido pelo usuario ou promovido, e campo ou valor invalido, dizendo o motivo de cada um. O usuario confirma em Inventario > Triagem por IA (decidir achado e registro continua so do usuario)."
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
        description = "Lista as propostas feitas pelo MCP e o estado de cada uma (pendente, confirmada, recusada, expirada). Só leitura. Para decidir use aplicar_proposta ou recusar_proposta (ou a janela do SBF)."
    )]
    fn listar_propostas(
        &self,
        Parameters(p): Parameters<PedidoDePropostas>,
    ) -> Result<String, String> {
        self.plano("listar_propostas", |l| l.listar_propostas(&p))
    }

    #[tool(
        description = "PROPÕE um compromisso novo (não aplica). Exige chave de idempotência. Nada muda até alguém confirmar (a janela do SBF ou aplicar_proposta); a resposta traz a simulação (meses afetados, antes e depois). Há backup automático antes da primeira escrita da sessão."
    )]
    fn propor_compromisso(
        &self,
        Parameters(p): Parameters<ComChave<PedidoNovoCompromisso>>,
    ) -> Result<String, String> {
        self.propor("propor_compromisso", &p.chave, Alt::Compromisso(p.pedido))
    }

    #[tool(
        description = "PROPÕE alterar um compromisso (valor, certeza, conta, centro...), só neste mês (so_este_mes, apenas o valor) ou daqui em diante (daqui_em_diante). Exige chave de idempotência. Nada muda até alguém confirmar (a janela do SBF ou aplicar_proposta). Há backup automático antes da primeira escrita."
    )]
    fn propor_alteracao(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeAlteracao>>,
    ) -> Result<String, String> {
        self.propor("propor_alteracao", &p.chave, Alt::Alteracao(p.pedido))
    }

    #[tool(
        description = "PROPÕE pausar um compromisso por um período (ate é exclusivo; sem ate, até retomar). Exige chave de idempotência. Nada muda até alguém confirmar (a janela do SBF ou aplicar_proposta). Há backup automático antes da primeira escrita."
    )]
    fn propor_pausa(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDePausa>>,
    ) -> Result<String, String> {
        self.propor("propor_pausa", &p.chave, Alt::Pausa(p.pedido))
    }

    #[tool(
        description = "PROPÕE retomar um compromisso pausado a partir de um mês. Exige chave de idempotência. Nada muda até alguém confirmar (a janela do SBF ou aplicar_proposta). Há backup automático antes da primeira escrita."
    )]
    fn propor_retomada(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeRetomada>>,
    ) -> Result<String, String> {
        self.propor("propor_retomada", &p.chave, Alt::Retomada(p.pedido))
    }

    #[tool(
        description = "PROPÕE marcar um compromisso como cancelado a partir de um mês (só no app; não cancela nada no fornecedor). Exige chave de idempotência. Nada muda até alguém confirmar (a janela do SBF ou aplicar_proposta). Há backup automático antes da primeira escrita."
    )]
    fn propor_cancelamento(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeCancelamento>>,
    ) -> Result<String, String> {
        self.propor("propor_cancelamento", &p.chave, Alt::Cancelamento(p.pedido))
    }

    #[tool(
        description = "Lista as regras de reconhecimento do cofre: id, nome, tipo de documento, versão em vigor, estado, execuções e taxa de acerto, e as que estão prontas para consolidar (consolidar_regra aplica). Só leitura."
    )]
    fn listar_regras(&self) -> Result<String, String> {
        self.regras("listar_regras", |r| r.listar())
    }

    #[tool(
        description = "Explica uma regra de reconhecimento: as versões (estado, autor, motivo), a definição e o script da versão em vigor e da última, e os exemplos (nome e resultado esperado). Só leitura."
    )]
    fn explicar_regra(&self, Parameters(p): Parameters<PedidoDeRegra>) -> Result<String, String> {
        self.regras("explicar_regra", |r| r.explicar(&p))
    }

    #[tool(
        description = "Roda os exemplos de uma versão de regra e diz, para cada um, se passou e o que divergiu. Só leitura: não ativa nem muda nada. Rode antes de ativar_regra: exemplo falhando bloqueia a ativação."
    )]
    fn testar_regra(&self, Parameters(p): Parameters<PedidoDeTeste>) -> Result<String, String> {
        self.regras("testar_regra", |r| r.testar(&p))
    }

    #[tool(
        description = "SIMULA uma regra sobre entradas dadas e mostra o que muda em relação às regras em vigor: uma versão salva (regra + versao) ou uma definição nova ainda não salva (definicao, script opcional). Não grava nada e não ativa nada."
    )]
    fn simular_regra(
        &self,
        Parameters(p): Parameters<PedidoDeSimulacaoDeRegra>,
    ) -> Result<String, String> {
        self.regras("simular_regra", |r| r.simular(&p))
    }

    #[tool(
        description = "PROPÕE uma versão nova de regra: salva um RASCUNHO com autor IA. PROPOR NÃO ATIVA: a versão em vigor continua a mesma; para ativar use ativar_regra (ou a janela do SBF). Confere o script antes de salvar. Há backup automático antes da primeira escrita da sessão."
    )]
    fn propor_regra(&self, Parameters(p): Parameters<PedidoDeProposta>) -> Result<String, String> {
        self.regras("propor_regra", |r| r.propor(&p))
    }

    #[tool(
        description = "PROPÕE um roteiro ESCRITO (Luau, para o navegador): valida o manifesto (hosts, rótulos do chaveiro, MFA, movimenta_dinheiro, classe) e salva um RASCUNHO com autor mcp. PROPOR NÃO ATIVA E NÃO EXECUTA: ao contrário das regras, o MCP NUNCA ativa, desativa, testa nem executa roteiro escrito; quem ativa é o usuário, só no terminal (`sbf roteiros escrito ativar`, com confirmação digitada) ou na janela. Manifesto inválido é recusado com o motivo; o mesmo conteúdo não duplica a versão. Fonte e manifesto até 64 KiB. Nunca ponha senha, CPF ou código: só rótulos do chaveiro. Há backup automático antes da primeira escrita da sessão."
    )]
    fn propor_roteiro_escrito(
        &self,
        Parameters(p): Parameters<PedidoDeRoteiroEscrito>,
    ) -> Result<String, String> {
        self.roteiros_escritos("propor_roteiro_escrito", |r| r.propor(&p))
    }

    #[tool(
        description = "Lista os roteiros escritos (Luau): nome, descrição, versão ativa, última versão e o estado da última (rascunho, testado, ativo ou aposentado). Só leitura."
    )]
    fn listar_roteiros_escritos(&self) -> Result<String, String> {
        self.roteiros_escritos("listar_roteiros_escritos", |r| r.listar())
    }

    #[tool(
        description = "Mostra um roteiro escrito: o manifesto, o fonte Luau, o estado, o autor, o resultado do teste e o histórico de estados. Sem versão, a mais nova (o campo ativa diz qual está em vigor). Só leitura; nunca devolve segredo (o manifesto guarda só rótulos do chaveiro)."
    )]
    fn ver_roteiro_escrito(
        &self,
        Parameters(p): Parameters<PedidoDeVerRoteiroEscrito>,
    ) -> Result<String, String> {
        self.roteiros_escritos("ver_roteiro_escrito", |r| r.ver(&p))
    }

    #[tool(
        description = "PROPÕE excluir (logicamente) um compromisso, com motivo; o histórico fica. Exige chave de idempotência. Nada muda até alguém confirmar (a janela do SBF ou aplicar_proposta). Há backup automático antes da primeira escrita."
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
        description = "PROPÕE ligar uma ocorrência a um movimento real (como sugestão de conciliação). Exige chave de idempotência. Nada muda até alguém confirmar (a janela do SBF ou aplicar_proposta). Há backup automático antes da primeira escrita."
    )]
    fn propor_conciliacao(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeConciliacao>>,
    ) -> Result<String, String> {
        self.propor("propor_conciliacao", &p.chave, Alt::Conciliacao(p.pedido))
    }

    #[tool(
        description = "PROPÕE uma exceção em uma ocorrência: adiar, dispensar (com motivo), pago_informado ou valor_so_deste_mes. Exige chave de idempotência. Nada muda até alguém confirmar (a janela do SBF ou aplicar_proposta). Há backup automático antes da primeira escrita."
    )]
    fn propor_excecao(
        &self,
        Parameters(p): Parameters<ComChave<PedidoDeExcecao>>,
    ) -> Result<String, String> {
        self.propor("propor_excecao", &p.chave, Alt::Excecao(p.pedido))
    }

    // ------------------------------------------------------------ D15: o MCP aplica direto

    #[tool(
        description = "APLICA AGORA (sem confirmação na janela) uma mudança num compromisso: renomear (nome), valor só neste mês (so_este_mes, apenas o valor) ou daqui em diante (daqui_em_diante), certeza, conta, centro, fornecedor, categoria, nota. Vai para o histórico com autor mcp. Para voltar atrás: desfazer. Backup automático do planejamento antes da primeira escrita da sessão."
    )]
    fn aplicar_alteracao(
        &self,
        Parameters(p): Parameters<PedidoDeAplicacao>,
    ) -> Result<String, String> {
        self.plano("aplicar_alteracao", |l| l.aplicar_alteracao(&p))
    }

    #[tool(
        description = "APLICA AGORA um compromisso novo (recusa o mesmo nome no mesmo grupo). Vai para o histórico com autor mcp; a resposta traz a simulação. Para voltar atrás: desfazer. Backup automático do planejamento antes da primeira escrita da sessão."
    )]
    fn criar_compromisso(
        &self,
        Parameters(p): Parameters<PedidoNovoCompromisso>,
    ) -> Result<String, String> {
        self.plano("criar_compromisso", |l| {
            l.aplicar("criar_compromisso", &Alt::Compromisso(p))
        })
    }

    #[tool(
        description = "APLICA AGORA a pausa de um compromisso por um período (ate é exclusivo; sem ate, até retomar). Histórico com autor mcp. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn pausar(&self, Parameters(p): Parameters<PedidoDePausa>) -> Result<String, String> {
        self.plano("pausar", |l| l.aplicar("pausar", &Alt::Pausa(p)))
    }

    #[tool(
        description = "APLICA AGORA a retomada de um compromisso pausado a partir de um mês. Histórico com autor mcp. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn retomar(&self, Parameters(p): Parameters<PedidoDeRetomada>) -> Result<String, String> {
        self.plano("retomar", |l| l.aplicar("retomar", &Alt::Retomada(p)))
    }

    #[tool(
        description = "APLICA AGORA o cancelamento de um compromisso a partir de um mês (só no app; não cancela nada no fornecedor). Histórico com autor mcp. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn cancelar(&self, Parameters(p): Parameters<PedidoDeCancelamento>) -> Result<String, String> {
        self.plano("cancelar", |l| l.aplicar("cancelar", &Alt::Cancelamento(p)))
    }

    #[tool(
        description = "APLICA AGORA uma exceção numa ocorrência: adiar, dispensar (com motivo), pago_informado ou valor_so_deste_mes. Histórico com autor mcp. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn excecao(&self, Parameters(p): Parameters<PedidoDeExcecao>) -> Result<String, String> {
        self.plano("excecao", |l| l.aplicar("excecao", &Alt::Excecao(p)))
    }

    #[tool(
        description = "APLICA AGORA a exclusão lógica de um compromisso, com motivo; o histórico fica e dá para restaurar. Autor mcp. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn excluir_logicamente(
        &self,
        Parameters(p): Parameters<PedidoDeExclusao>,
    ) -> Result<String, String> {
        self.plano("excluir_logicamente", |l| {
            l.aplicar("excluir_logicamente", &Alt::ExclusaoLogica(p))
        })
    }

    #[tool(
        description = "APLICA AGORA a conciliação CONFIRMADA de uma ocorrência com um ou VÁRIOS movimentos (de 1 a 20; tudo ou nada). Autor mcp no histórico. Para voltar atrás: desfazer (um por movimento, o último primeiro) ou desconciliar. Backup automático antes da primeira escrita."
    )]
    fn conciliar(&self, Parameters(p): Parameters<PedidoDeConciliar>) -> Result<String, String> {
        self.plano("conciliar", |l| l.conciliar(&p))
    }

    #[tool(
        description = "APLICA AGORA o desfazer de uma conciliação (um movimento da ocorrência): exclusão lógica, com histórico e autor mcp. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn desconciliar(
        &self,
        Parameters(p): Parameters<PedidoDeDesconciliar>,
    ) -> Result<String, String> {
        self.plano("desconciliar", |l| l.desconciliar(&p))
    }

    #[tool(
        description = "APLICA AGORA uma proposta pendente (veja listar_propostas), revalidando a versão de base, com autor mcp no histórico. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn aplicar_proposta(
        &self,
        Parameters(p): Parameters<PedidoDeAplicarProposta>,
    ) -> Result<String, String> {
        self.plano("aplicar_proposta", |l| l.aplicar_proposta(&p))
    }

    #[tool(
        description = "RECUSA uma proposta pendente, com motivo. Não aplica nada. Backup automático antes da primeira escrita."
    )]
    fn recusar_proposta(
        &self,
        Parameters(p): Parameters<PedidoDeRecusaDeProposta>,
    ) -> Result<String, String> {
        self.plano("recusar_proposta", |l| l.recusar_proposta(&p))
    }

    #[tool(
        description = "Lista os modelos de compromisso (o molde que vários compromissos compartilham: parâmetros + o Cálculo em Luau), com a versão e o checksum do Cálculo, o script e quais compromissos usam cada um, com os valores e vigências. De fábrica: \"Diarista por dia da semana\" (parâmetros diaria em centavos e dia_da_semana; previsto = diária × quantas vezes aquele dia cai no mês). Só leitura."
    )]
    fn listar_modelos(&self) -> Result<String, String> {
        self.plano("listar_modelos", |l| l.listar_modelos())
    }

    #[tool(
        description = "APLICA AGORA um modelo de compromisso novo: parâmetros declarados (centavos, dia_da_semana, inteiro, texto) e o Cálculo em Luau (função calcular(entrada, ajuda) que devolve {valor_centavos, quantidade?, explicacao}). O script é conferido antes. Histórico com autor mcp. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn criar_modelo(
        &self,
        Parameters(p): Parameters<PedidoDeCriarModelo>,
    ) -> Result<String, String> {
        self.plano("criar_modelo", |l| l.criar_modelo(&p))
    }

    #[tool(
        description = "APLICA AGORA a ligação de um compromisso a um modelo (id ou nome do modelo), com os valores dos parâmetros e a vigência de cada um (desde AAAA-MM). O previsto de cada mês passa a ser o resultado do Cálculo. Histórico com autor mcp. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn aplicar_modelo(
        &self,
        Parameters(p): Parameters<PedidoDeAplicarModelo>,
    ) -> Result<String, String> {
        self.plano("aplicar_modelo", |l| l.aplicar_modelo(&p))
    }

    #[tool(
        description = "APLICA AGORA um valor de parâmetro com vigência (desde AAAA-MM) num compromisso que já usa um modelo (ex.: diaria 21000 desde 2026-05). Os meses anteriores não mudam. Histórico com autor mcp. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn definir_parametro(
        &self,
        Parameters(p): Parameters<PedidoDeDefinirParametro>,
    ) -> Result<String, String> {
        self.plano("definir_parametro", |l| l.definir_parametro(&p))
    }

    #[tool(
        description = "APLICA AGORA a quantidade (ex.: 6 diárias) de UM mês só, num compromisso com modelo: o Cálculo usa esta quantidade naquele mês e o resto segue o cálculo. Aparece no Rastro com autor mcp. Para voltar atrás: desfazer. Backup automático antes da primeira escrita."
    )]
    fn ajustar_quantidade(
        &self,
        Parameters(p): Parameters<PedidoDeAjustarQuantidade>,
    ) -> Result<String, String> {
        self.plano("ajustar_quantidade", |l| l.ajustar_quantidade(&p))
    }

    #[tool(
        description = "DESFAZ uma alteração que o MCP aplicou: a última ainda não desfeita, ou a de alteracao_id (só as do mcp). O Desfazer vai para o histórico com autor mcp. Não desfaz o que o usuário fez. Backup automático antes da primeira escrita."
    )]
    fn desfazer(&self, Parameters(p): Parameters<PedidoDeDesfazer>) -> Result<String, String> {
        self.plano("desfazer", |l| l.desfazer(&p))
    }

    #[tool(
        description = "ATIVA AGORA uma versão de regra de reconhecimento (autor mcp em historico_de_estado). Os exemplos rodam antes: exemplo falhando BLOQUEIA a ativação (vale para o MCP também). A versão em vigor anterior vira desativada. Para voltar atrás: reverter_regra. Backup automático do regras.sqlite antes da primeira escrita da sessão."
    )]
    fn ativar_regra(&self, Parameters(p): Parameters<PedidoDeVersao>) -> Result<String, String> {
        self.regras("ativar_regra", |r| r.ativar(&p))
    }

    #[tool(
        description = "DESATIVA AGORA a versão em vigor de uma regra (autor mcp no histórico). Para voltar atrás: ativar_regra com a versão. Backup automático antes da primeira escrita."
    )]
    fn desativar_regra(&self, Parameters(p): Parameters<PedidoDeRegra>) -> Result<String, String> {
        self.regras("desativar_regra", |r| r.desativar(&p))
    }

    #[tool(
        description = "REVERTE AGORA uma regra à versão que estava em vigor antes (rollback; reverter de novo volta à de agora). Autor mcp no histórico. Backup automático antes da primeira escrita."
    )]
    fn reverter_regra(&self, Parameters(p): Parameters<PedidoDeRegra>) -> Result<String, String> {
        self.regras("reverter_regra", |r| r.reverter(&p))
    }

    #[tool(
        description = "CONSOLIDA AGORA uma versão ativa que já tem as aceitas seguidas necessárias (veja listar_regras); a consolidada concilia sozinha só com as três provas, com Desfazer na janela. Autor mcp no histórico. Backup automático antes da primeira escrita."
    )]
    fn consolidar_regra(
        &self,
        Parameters(p): Parameters<PedidoDeVersao>,
    ) -> Result<String, String> {
        self.regras("consolidar_regra", |r| r.consolidar(&p))
    }

    #[tool(
        description = "COLETA AGORA o Gmail do usuario: baixa os anexos (PDF, imagem, XML) para <cofre>/entrada/gmail/, registra no inventario com origem gmail:<mensagem>:<parte>, deduplica por sha256 e passa cada documento novo pelas regras de reconhecimento; e-mail sem anexo vira entrada Mensagem. Aplica direto (D15), sem confirmacao; nada vai para o planejamento (as regras so propoem). Checkpoint automatico: coletar de novo so traz o que e novo. Devolve so contagens. Backup automatico do inventario e das regras antes da primeira escrita da sessao."
    )]
    fn gmail_coletar(
        &self,
        Parameters(p): Parameters<PedidoDeColetaDoGmail>,
    ) -> Result<String, String> {
        responder("gmail_coletar", self.nucleo.gmail_coletar(&p.into()))
    }

    #[tool(
        description = "Lista as conversas (grupos, canais e privadas) da conta do Telegram do usuário, com id, nome, tipo e se já é seguida. atualizar=true pergunta ao Telegram agora; sem isso vale a última lista guardada. Só leitura: nada é enviado, apagado nem marcado como lido."
    )]
    fn telegram_conversas(
        &self,
        Parameters(p): Parameters<PedidoDeConversas>,
    ) -> Result<String, String> {
        responder(
            "telegram_conversas",
            self.nucleo
                .telegram()
                .and_then(|t| crate::telegram::conversas(t, &p)),
        )
    }

    #[tool(
        description = "APLICA AGORA: passa a seguir (ou, com deixar=true, deixa de seguir) uma conversa do Telegram, pelo id ou pelo nome. Só as conversas seguidas são coletadas. Para voltar atrás: telegram_seguir com deixar=true."
    )]
    fn telegram_seguir(&self, Parameters(p): Parameters<PedidoDeSeguir>) -> Result<String, String> {
        responder(
            "telegram_seguir",
            self.nucleo
                .telegram()
                .and_then(|t| crate::telegram::seguir(t, &p)),
        )
    }

    #[tool(
        description = "COLETA AGORA as mídias novas (fotos, documentos e PDFs) das conversas seguidas do Telegram: baixa para <cofre>/entrada/telegram/<conversa>/, registra no inventário como achado com origem telegram:<conversa>:<mensagem> (o mesmo arquivo em outra mensagem é só uma origem a mais) e roda o Reconhecimento em cada arquivo novo. conversa limita a uma; desde (AAAA-MM-DD) vale só na primeira coleta de cada conversa. Só leitura no Telegram."
    )]
    fn telegram_coletar(
        &self,
        Parameters(p): Parameters<PedidoDeColeta>,
    ) -> Result<String, String> {
        responder(
            "telegram_coletar",
            self.nucleo
                .telegram()
                .and_then(|t| crate::telegram::coletar(t, &p)),
        )
    }

    #[tool(
        description = "Faz AGORA o backup de todos os bancos do cofre (inventário, planejamento, regras...) em <cofre>/backups/mcp/, com a API de backup do SQLite (consistente com o WAL); guarda os 20 mais recentes por banco. Os bancos que o MCP escreve já são copiados sozinhos antes da primeira escrita de cada sessão."
    )]
    fn fazer_backup(&self) -> Result<String, String> {
        responder("fazer_backup", self.nucleo.fazer_backup())
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

    fn regras(
        &self,
        nome: &str,
        f: impl FnOnce(&Regras) -> Result<Value, String>,
    ) -> Result<String, String> {
        responder(nome, self.nucleo.regras().and_then(f))
    }

    fn roteiros_escritos(
        &self,
        nome: &str,
        f: impl FnOnce(&RoteirosEscritos) -> Result<Value, String>,
    ) -> Result<String, String> {
        responder(nome, self.nucleo.roteiros_escritos().and_then(f))
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
    instructions = "Inventario financeiro, livro-caixa e controle mensal do usuario. DESDE A D15 (2026-11-18) este servidor tem CONTROLE TOTAL: as ferramentas aplicar_*, criar_compromisso, pausar, retomar, cancelar, excecao, excluir_logicamente, conciliar, desconciliar, recusar_proposta, criar_modelo, aplicar_modelo, definir_parametro, ajustar_quantidade e as de regra (ativar_regra, desativar_regra, reverter_regra, consolidar_regra) APLICAM na hora, sem confirmacao na janela, com autor mcp no historico e no Rastro (gmail_coletar e telegram_coletar/telegram_seguir tambem aplicam direto: coletam o Gmail e o Telegram para o cofre); desfazer volta atras a ultima alteracao do mcp (reverter_regra para regras). As ferramentas propor_* continuam so propondo. Exemplo de regra falhando bloqueia a ativacao (vale para o mcp tambem). ROTEIROS ESCRITOS (Luau, navegador com credenciais) sao diferentes das regras: o mcp so PROPOE (propor_roteiro_escrito cria um RASCUNHO com autor mcp) e consulta (listar_roteiros_escritos, ver_roteiro_escrito); NAO existe ferramenta que ative, desative, teste ou execute roteiro escrito, e ativar e so do usuario, no terminal (sbf roteiros escrito ativar) ou na janela. Antes da primeira escrita de cada banco na sessao ha backup automatico em <cofre>/backups/mcp/ (e fazer_backup sob demanda); se o backup falha, a escrita e recusada. A triagem do inventario continua so do usuario: sugerir apenas PROPOE classificacoes (Inventario > Triagem por IA). Valores em centavos. Comece por inventario_resumo; simule antes de aplicar (simular_mudanca, simular_regra, testar_regra); leia fornecedores e decisoes_recentes para seguir o criterio do usuario."
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
