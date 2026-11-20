//! As ferramentas de Rita (spec 15, "Ferramentas de Rita"): poucas, de saída curta, e nenhuma
//! que ative, consolide, reverta, vincule, concilie ou escreva fora do `regras.sqlite`.
//!
//! Quem impõe o teto de chamadas é a [`Sessao`], não o modelo: a chamada de número 9 é recusada
//! com texto fixo, venha de onde vier (laço próprio ou servidor MCP do `claude -p`). Uma
//! ferramenta fora da lista também é recusada (e conta).

use std::cmp::Reverse;

use regex::RegexBuilder;
use sbf_mcp::backup::{Backups, Qual};
use sbf_regras::catalogo::{Catalogo, NovaVersao, VersaoDeRegra};
use sbf_regras::definicao::{Condicao, Definicao, Formato, normalizar};
use sbf_regras::{Autor, Estado};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::contexto::Contexto;
use crate::diagnostico::linhas_parecidas;
use crate::pre_teste::{PreTeste, pre_testar, pre_testar_versao};
use crate::texto::cortar;

/// O teto de chamadas de ferramenta por pedido (D11).
pub const MAXIMO_DE_CHAMADAS: usize = 8;
/// Quantos caracteres do texto uma página de `ver_item` traz (e o primeiro prompt também).
pub const TAMANHO_DA_PAGINA: usize = 20_000;
const MAX_NOME: usize = 120;
/// O `motivo` da versão: o pedido (e, num refinamento, o original e o de agora).
pub const MAX_MOTIVO: usize = 1000;
const SEPARADOR_DO_MOTIVO: &str = " · ";
const MAX_SCRIPT_NA_LEITURA: usize = 1500;
const MAX_PADROES_NA_LISTA: usize = 20;
const MAX_TITULOS: usize = 30;
const MAX_CASAMENTOS: usize = 5;
const LIMITE_DO_REGEX: usize = 1 << 20;

/// Por que `propor_padrao` não gravou nem ensaiou.
enum Falha {
    Erro(String),
    /// Definição idêntica a uma tentativa anterior: recusada sem rodar.
    Repetida(String),
}

/// Os nomes das ferramentas de Rita, na ordem em que aparecem para o modelo.
pub const FERRAMENTAS: [&str; 8] = [
    "ver_item",
    "rastro_do_item",
    "listar_padroes",
    "ver_padrao",
    "testar_regex",
    "propor_padrao",
    "pre_testar",
    "titulos_da_lista",
];

/// Uma ferramenta que Rita chamou, para a tela e a CLI mostrarem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChamadaFeita {
    pub numero: usize,
    pub nome: String,
    /// Os argumentos, curtos.
    pub argumentos: String,
    /// A primeira linha do resultado, curta.
    pub resultado: String,
    pub recusada: bool,
}

/// Um rascunho que Rita gravou, com o pré-teste dele.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RascunhoCriado {
    pub regra: String,
    pub versao: u32,
    pub nome: String,
    /// `false`: já existia uma versão igual (nada novo foi gravado).
    pub criou: bool,
    /// O rascunho foi proposto (ou re-proposto) **neste pedido**. Só estes contam para "para no
    /// primeiro que passa": um `pre_testar` de uma versão antiga não encerra o pedido.
    #[serde(default)]
    pub neste_pedido: bool,
    pub pre_teste: PreTeste,
}

/// Uma definição que Rita ensaiou neste pedido (`propor_padrao` ou `pre_testar` com a definição
/// inline). Não vira versão no catálogo: só a que passa é gravada, ou, no fim do pedido sem
/// nenhuma que passe, a melhor delas (uma só).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tentativa {
    /// A ordem no pedido, a partir de 1.
    pub numero: usize,
    pub regra: String,
    pub nome: String,
    pub definicao: Definicao,
    pub script: Option<String>,
    /// A definição normalizada: duas tentativas com a mesma chave são a mesma.
    pub chave: String,
    pub pre_teste: PreTeste,
}

impl Tentativa {
    /// O quanto a tentativa avançou: passou > casa o item > menos condições falhando > menos
    /// campos faltando.
    fn progresso(&self) -> (bool, bool, Reverse<usize>, Reverse<usize>) {
        (
            self.pre_teste.passou,
            self.pre_teste.item.casa,
            Reverse(self.pre_teste.item.condicoes_falhando),
            Reverse(self.pre_teste.item.faltam.len()),
        )
    }
}

/// A definição como chave de comparação: os `contem` normalizados e as listas em ordem
/// canônica, mais o script.
fn chave_da_definicao(def: &Definicao, script: Option<&str>) -> String {
    let mut d = def.clone();
    for lista in [
        &mut d.detectar.todos,
        &mut d.detectar.algum,
        &mut d.detectar.nenhum,
    ] {
        for c in lista.iter_mut() {
            if let Condicao::Contem(t) = c {
                *t = normalizar(t);
            }
        }
        lista.sort_by_cached_key(|c| serde_json::to_string(c).unwrap_or_default());
    }
    d.obrigatorios.sort();
    format!(
        "{}\u{0}{}",
        serde_json::to_string(&d).unwrap_or_default(),
        script.map_or("", str::trim)
    )
}

/// `\\\\s` (barra dupla antes de uma classe) quase sempre é escape a mais do modelo: o regex
/// literal que casaria uma barra seguida de `s` não existe em documento. Devolve o regex com
/// uma barra só, se isso o muda e ele ainda compila.
fn sem_barras_duplas(regex: &str) -> Option<String> {
    static DUPLA: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\\\\([sdwSDWbB$.])").unwrap_or_else(|_| unreachable!())
    });
    if !DUPLA.is_match(regex) {
        return None;
    }
    let novo = DUPLA
        .replace_all(regex, |c: &regex::Captures<'_>| format!("\\{}", &c[1]))
        .into_owned();
    RegexBuilder::new(&novo)
        .size_limit(LIMITE_DO_REGEX)
        .dfa_size_limit(LIMITE_DO_REGEX)
        .build()
        .ok()
        .map(|_| novo)
}

/// Conserta as barras duplas dos regexes da definição e devolve um aviso por regex mexido.
fn consertar_barras(def: &mut Definicao) -> Vec<String> {
    let mut avisos = Vec::new();
    let mut aviso = |onde: String| {
        avisos.push(format!(
            "o regex {onde} veio com barra invertida dupla (`\\\\s`): usei uma só (`\\s`); mande a barra uma vez só"
        ));
    };
    for (nome, e) in &mut def.extrair {
        if let Some(novo) = sem_barras_duplas(&e.regex) {
            e.regex = novo;
            aviso(format!("do campo `{nome}`"));
        }
    }
    for (i, c) in def.detectar.todos.iter_mut().enumerate() {
        if let Condicao::Regex(p) = c
            && let Some(novo) = sem_barras_duplas(p)
        {
            *p = novo;
            aviso(format!("da condição todos[{i}]"));
        }
    }
    avisos
}

/// Os modelos que mandam a definição como texto escrevem `\\d` e `\\s` dentro do JSON, que não
/// são escapes válidos: dobra a barra que não precede um escape do JSON.
fn reparar_escapes(json: &str) -> String {
    let mut saida = String::with_capacity(json.len() + 8);
    let mut chars = json.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            saida.push(c);
            continue;
        }
        match chars.peek() {
            Some('"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' | 'u') => {
                saida.push('\\');
                if let Some(proximo) = chars.next() {
                    saida.push(proximo);
                }
            }
            _ => saida.push_str("\\\\"),
        }
    }
    saida
}

/// Um id de padrão a partir do tipo documental (quando o modelo ensaia sem dar `regra`).
fn id_do_tipo(tipo: &str) -> String {
    let id: String = tipo
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let id = id.trim_matches('-').to_owned();
    if id.is_empty() {
        "padrao".to_owned()
    } else {
        id
    }
}

/// O `motivo` da versão: o pedido de agora; num refinamento (a conversa já tinha um pedido), o
/// pedido original + " · " + o de agora. Até [`MAX_MOTIVO`] caracteres: o original ocupa no
/// máximo 40% e o de agora o resto.
pub fn motivo_do_pedido(original: Option<&str>, atual: &str) -> String {
    let atual = atual.trim();
    let Some(original) = original.map(str::trim).filter(|o| !o.is_empty()) else {
        return cortar(atual, MAX_MOTIVO);
    };
    let original = cortar(original, MAX_MOTIVO * 2 / 5);
    let resto = MAX_MOTIVO - original.chars().count() - SEPARADOR_DO_MOTIVO.chars().count();
    format!("{original}{SEPARADOR_DO_MOTIVO}{}", cortar(atual, resto))
}

fn erro(m: impl AsRef<str>) -> String {
    json!({"erro": m.as_ref()}).to_string()
}

/// A definição das ferramentas, para qualquer provedor (e para o servidor MCP).
pub fn definicoes() -> Vec<sbf_ia::DefinicaoDeFerramenta> {
    let def = |nome: &str, descricao: &str, parametros: Value| sbf_ia::DefinicaoDeFerramenta {
        nome: nome.to_owned(),
        descricao: descricao.to_owned(),
        parametros,
    };
    vec![
        def(
            "ver_item",
            "O texto do item na tela, por páginas de 20 mil caracteres. A página 1 já está no pedido: só chame para ler além do corte.",
            json!({"type": "object", "properties": {
                "pagina": {"type": "integer", "description": "Número da página, a partir de 1."}
            }}),
        ),
        def(
            "rastro_do_item",
            "Quais padrões se aplicam ao item, quais casaram e qual venceu (ou se há empate).",
            json!({"type": "object", "properties": {}}),
        ),
        def(
            "listar_padroes",
            "Lista os padrões existentes (nome, tipo, estado, condições). Filtre por estado ou por texto.",
            json!({"type": "object", "properties": {
                "estado": {"type": "string", "enum": ["rascunho", "ativa", "consolidada", "desativada"]},
                "texto": {"type": "string", "description": "Parte do id, do nome, do tipo ou das condições."}
            }}),
        ),
        def(
            "ver_padrao",
            "A definição completa de uma versão de um padrão (condições, campos, exemplos).",
            json!({"type": "object", "properties": {
                "regra": {"type": "string"},
                "versao": {"type": "integer", "description": "Padrão: a última."}
            }, "required": ["regra"]}),
        ),
        def(
            "testar_regex",
            "Roda um regex no texto do item e devolve até 5 casamentos, cada um com a linha onde está e o valor do grupo. Use antes de `propor_padrao` para acertar o regex de um campo; é barato.",
            json!({"type": "object", "properties": {
                "regex": {"type": "string", "description": "O regex (sintaxe Rust), com a barra invertida uma vez só: \\d, \\s."},
                "grupo": {"type": "integer", "description": "Grupo de captura que vale (padrão: 1, ou 0 se o regex não tem grupo)."},
                "formato": {"type": "string", "enum": ["texto", "centavos", "data", "competencia", "inteiro", "minusculas"], "description": "Opcional: mostra também o valor depois do formato."}
            }, "required": ["regex"]}),
        ),
        def(
            "propor_padrao",
            "Testa a definição (pré-teste) e SÓ GRAVA o rascunho de padrão se ele passar. Se `passou` vier false, NADA foi gravado: corrija o que o `diagnostico` aponta e chame de novo com o mesmo `regra`. Definição idêntica a uma tentativa anterior é recusada. Nunca ativa.",
            json!({"type": "object", "properties": {
                "regra": {"type": "string", "description": "Id: minúsculas, dígitos e hífen."},
                "nome": {"type": "string", "description": "Nome para mostrar (até 120 caracteres)."},
                "definicao": {"type": "object", "description": "A definição no formato das instruções: tipo_documental, aplica_a, prioridade, detectar{todos,algum,nenhum}, extrair{campo:{regex,grupo,formato}}, obrigatorios, propor."},
                "script": {"type": "string", "description": "Script Luau opcional; só para o que a definição não resolve."}
            }, "required": ["regra", "nome", "definicao"]}),
        ),
        def(
            "pre_testar",
            "Pré-testa e NÃO grava: ou uma versão já gravada (`regra` e `versao`), ou uma `definicao` inline (ensaio). Mostra o item, os exemplos e os documentos parecidos do cofre. Só leitura.",
            json!({"type": "object", "properties": {
                "regra": {"type": "string"},
                "versao": {"type": "integer", "description": "Com `regra`: a versão gravada a pré-testar."},
                "definicao": {"type": "object", "description": "Alternativa a `versao`: a definição a ensaiar, no mesmo formato de `propor_padrao`."},
                "nome": {"type": "string"}
            }}),
        ),
        def(
            "titulos_da_lista",
            "Os títulos dos itens da lista na tela. Só use se o usuário pediu para olhar os outros itens.",
            json!({"type": "object", "properties": {}}),
        ),
    ]
}

/// O placar de uma sessão (o servidor MCP do `claude -p` grava isto para o processo pai ler).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EstadoDaSessao {
    pub usadas: usize,
    pub maximo: usize,
    pub estourou: bool,
    pub chamadas: Vec<ChamadaFeita>,
    pub rascunhos: Vec<RascunhoCriado>,
    /// Tudo o que foi ensaiado neste pedido (grave ou não).
    #[serde(default)]
    pub tentativas: Vec<Tentativa>,
}

impl EstadoDaSessao {
    /// Algum rascunho **deste pedido** passou no pré-teste (D11: o primeiro que passa encerra).
    pub fn passou(&self) -> bool {
        self.rascunhos
            .iter()
            .any(|r| r.neste_pedido && r.pre_teste.passou)
    }

    /// O índice do melhor rascunho: o primeiro deste pedido que passou; senão o primeiro que
    /// passou; senão o último em que o item casa; senão o último.
    pub fn melhor(&self) -> Option<usize> {
        self.rascunhos
            .iter()
            .position(|r| r.neste_pedido && r.pre_teste.passou)
            .or_else(|| self.rascunhos.iter().position(|r| r.pre_teste.passou))
            .or_else(|| self.rascunhos.iter().rposition(|r| r.pre_teste.item.casa))
            .or_else(|| self.rascunhos.len().checked_sub(1))
    }
}

/// Uma sessão de ferramentas: o contexto, o pedido (vira o `motivo`), o autor e o placar.
pub struct Sessao {
    ctx: Contexto,
    pedido: String,
    modelo: String,
    maximo: usize,
    backups: Backups,
    usadas: usize,
    estourou: bool,
    pub chamadas: Vec<ChamadaFeita>,
    pub rascunhos: Vec<RascunhoCriado>,
    pub tentativas: Vec<Tentativa>,
    /// `encerrar` já rodou.
    encerrada: bool,
    /// Uma gravação falhou neste pedido: `encerrar` não tenta de novo por conta própria.
    gravacao_falhou: bool,
    /// Avisos da chamada em curso (entram na saída dela como `avisos`).
    avisos: Vec<String>,
}

impl std::fmt::Debug for Sessao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sessao")
            .field("usadas", &self.usadas)
            .field("rascunhos", &self.rascunhos.len())
            .finish_non_exhaustive()
    }
}

impl Sessao {
    /// `pedido` é o texto que vira o `motivo` da versão (veja [`motivo_do_pedido`]) e `modelo`
    /// o nome do modelo (vira o rótulo `assistente (<modelo>)` no histórico da versão).
    pub fn nova(ctx: Contexto, pedido: &str, modelo: &str, backups: Backups) -> Self {
        Self {
            ctx,
            pedido: cortar(pedido.trim(), MAX_MOTIVO),
            modelo: modelo.to_owned(),
            maximo: MAXIMO_DE_CHAMADAS,
            backups,
            usadas: 0,
            estourou: false,
            chamadas: Vec::new(),
            rascunhos: Vec::new(),
            tentativas: Vec::new(),
            encerrada: false,
            gravacao_falhou: false,
            avisos: Vec::new(),
        }
    }

    /// Troca o teto (só os testes).
    #[must_use]
    pub fn com_maximo(mut self, maximo: usize) -> Self {
        self.maximo = maximo;
        self
    }

    pub fn usadas(&self) -> usize {
        self.usadas
    }

    pub fn maximo(&self) -> usize {
        self.maximo
    }

    /// O teto foi atingido (a chamada seguinte foi ou será recusada).
    pub fn estourou(&self) -> bool {
        self.estourou || self.usadas >= self.maximo
    }

    /// Algum rascunho **proposto neste pedido** passou no pré-teste: dali em diante toda
    /// chamada restante é recusada sem executar ([`Self::recusar_depois_de_passar`]).
    pub fn passou(&self) -> bool {
        self.estado().passou()
    }

    /// A recusa de uma chamada que veio depois de um rascunho passar (D11): não executa, não
    /// gasta do teto e fica no placar como recusada. É a mesma para o laço próprio e para o
    /// servidor MCP do `claude -p`.
    pub fn recusar_depois_de_passar(&mut self, nome: &str, argumentos: &Value) -> String {
        let msg = erro(
            "já existe um rascunho que passou no pré-teste: pare e responda ao usuário, sem mais ferramentas",
        );
        let numero = self.usadas + 1;
        self.registrar(numero, nome, argumentos, &msg, true);
        msg
    }

    /// Uma cópia do placar.
    pub fn estado(&self) -> EstadoDaSessao {
        EstadoDaSessao {
            usadas: self.usadas,
            maximo: self.maximo,
            estourou: self.estourou(),
            chamadas: self.chamadas.clone(),
            rascunhos: self.rascunhos.clone(),
            tentativas: self.tentativas.clone(),
        }
    }

    /// Retoma o placar de uma sessão que rodou em outro processo (o servidor MCP do `claude -p`
    /// grava o estado; o processo pai o relê para [`Self::encerrar`]).
    pub fn restaurar(&mut self, e: EstadoDaSessao) {
        self.usadas = e.usadas;
        self.estourou = e.estourou;
        self.chamadas = e.chamadas;
        self.rascunhos = e.rascunhos;
        self.tentativas = e.tentativas;
    }

    /// A melhor das tentativas que não viraram versão (a que passou, senão a que mais avançou;
    /// no empate, a mais recente).
    pub fn melhor_tentativa(&self) -> Option<&Tentativa> {
        self.tentativas.iter().max_by_key(|t| t.progresso())
    }

    /// O fim do pedido (o modelo parou ou o teto estourou). Se nenhum rascunho foi gravado
    /// neste pedido, grava **uma** versão de rascunho com a melhor tentativa, que quase sempre
    /// não passou: quem chama diz isso ao usuário. Roda uma vez só.
    ///
    /// # Errors
    /// O texto de por que a versão não foi gravada.
    pub fn encerrar(&mut self) -> Result<(), String> {
        if std::mem::replace(&mut self.encerrada, true) {
            return Ok(());
        }
        if self.gravacao_falhou || self.rascunhos.iter().any(|r| r.neste_pedido) {
            return Ok(());
        }
        let Some(t) = self.melhor_tentativa().cloned() else {
            return Ok(());
        };
        self.gravar(&t.regra, &t.nome, t.definicao, t.script)
            .map(|_| ())
    }

    /// Executa uma chamada. Conta no teto; acima dele, recusa. Nunca entra em pânico.
    pub fn executar(&mut self, nome: &str, argumentos: &Value) -> String {
        let numero = self.usadas + 1;
        if self.usadas >= self.maximo {
            self.estourou = true;
            let msg = erro(format!(
                "limite de {} chamadas de ferramenta atingido neste pedido: responda agora, sem chamar mais nenhuma",
                self.maximo
            ));
            self.registrar(numero, nome, argumentos, &msg, true);
            return msg;
        }
        self.usadas += 1;
        self.avisos.clear();
        let (saida, recusada) = match nome {
            "ver_item" => (self.ver_item(argumentos), false),
            "rastro_do_item" => (self.rastro_do_item(), false),
            "listar_padroes" => (self.listar_padroes(argumentos), false),
            "ver_padrao" => (self.ver_padrao(argumentos), false),
            "testar_regex" => (self.testar_regex(argumentos), false),
            "propor_padrao" => self.propor_padrao(argumentos),
            "pre_testar" => (self.pre_testar(argumentos), false),
            "titulos_da_lista" => (self.titulos_da_lista(), false),
            outra => (
                erro(format!(
                    "ferramenta desconhecida `{}`: use só {}",
                    cortar(outra, 40),
                    FERRAMENTAS.join(", ")
                )),
                true,
            ),
        };
        let saida = self.com_avisos(saida);
        self.registrar(numero, nome, argumentos, &saida, recusada);
        saida
    }

    /// Acrescenta `avisos` à saída (JSON) da chamada, se houve algum.
    fn com_avisos(&mut self, saida: String) -> String {
        if self.avisos.is_empty() {
            return saida;
        }
        let avisos = std::mem::take(&mut self.avisos);
        match serde_json::from_str::<Value>(&saida) {
            Ok(Value::Object(mut o)) => {
                o.insert("avisos".to_owned(), json!(avisos));
                Value::Object(o).to_string()
            }
            _ => saida,
        }
    }

    fn registrar(&mut self, numero: usize, nome: &str, args: &Value, saida: &str, recusada: bool) {
        self.chamadas.push(ChamadaFeita {
            numero,
            nome: cortar(nome, 40),
            argumentos: cortar(&args.to_string(), 160),
            resultado: cortar(saida.lines().next().unwrap_or(""), 200),
            recusada,
        });
    }

    fn catalogo(&self) -> Result<Catalogo, String> {
        catalogo_do(&self.ctx)
    }

    // ---- ver_item ----

    fn ver_item(&self, a: &Value) -> String {
        let item = &self.ctx.tela.item;
        let Some(texto) = item.texto.as_deref().filter(|t| !t.trim().is_empty()) else {
            return erro("este item não tem texto guardado");
        };
        let pagina = a
            .get("pagina")
            .and_then(Value::as_u64)
            .and_then(|p| usize::try_from(p).ok())
            .filter(|p| *p >= 1)
            .unwrap_or(1);
        let total = texto.chars().count().div_ceil(TAMANHO_DA_PAGINA).max(1);
        if pagina > total {
            return erro(format!("o texto tem {total} página(s)"));
        }
        let trecho: String = texto
            .chars()
            .skip((pagina - 1) * TAMANHO_DA_PAGINA)
            .take(TAMANHO_DA_PAGINA)
            .collect();
        // Quem entrega a saída ao modelo a põe num bloco de dados (`prompt::saida_de_ferramenta`).
        format!("página {pagina} de {total}.\n{trecho}")
    }

    // ---- rastro_do_item ----

    fn rastro_do_item(&self) -> String {
        match rastro_do_item(&self.ctx) {
            Ok(v) => v.to_string(),
            Err(e) => erro(e),
        }
    }

    // ---- listar_padroes / ver_padrao ----

    fn listar_padroes(&self, a: &Value) -> String {
        let c = match self.catalogo() {
            Ok(c) => c,
            Err(e) => return erro(e),
        };
        let estado = a.get("estado").and_then(Value::as_str);
        let texto = a
            .get("texto")
            .and_then(Value::as_str)
            .map(str::to_lowercase)
            .filter(|t| !t.is_empty());
        let Ok(regras) = c.regras() else {
            return erro("falha ao ler os padrões");
        };
        let mut itens = Vec::new();
        let mut total = 0_usize;
        for r in regras {
            let Ok(versoes) = c.versoes(&r.id) else {
                continue;
            };
            let Some(ultima) = versoes.last() else {
                continue;
            };
            let mostrada = versoes
                .iter()
                .rev()
                .find(|v| matches!(v.estado, Estado::Ativa | Estado::Consolidada))
                .unwrap_or(ultima);
            let est = estado_texto(mostrada.estado);
            let condicoes = condicoes_curtas(&mostrada.definicao);
            if estado.is_some_and(|e| e != est) {
                continue;
            }
            if let Some(t) = &texto
                && !format!("{} {} {} {}", r.id, r.nome, r.tipo_documental, condicoes)
                    .to_lowercase()
                    .contains(t)
            {
                continue;
            }
            total += 1;
            if itens.len() < MAX_PADROES_NA_LISTA {
                itens.push(json!({
                    "regra": r.id, "nome": cortar(&r.nome, 60), "tipo": r.tipo_documental,
                    "estado": est, "versao": mostrada.versao,
                    "prioridade": mostrada.definicao.prioridade, "condicoes": condicoes,
                }));
            }
        }
        json!({"total": total, "mostrados": itens.len(), "padroes": itens}).to_string()
    }

    fn ver_padrao(&self, a: &Value) -> String {
        let Some(regra) = a.get("regra").and_then(Value::as_str) else {
            return erro("falta `regra`");
        };
        let c = match self.catalogo() {
            Ok(c) => c,
            Err(e) => return erro(e),
        };
        let Ok(versoes) = c.versoes(regra.trim()) else {
            return erro("falha ao ler o padrão");
        };
        let alvo = match a.get("versao").and_then(Value::as_u64) {
            Some(n) => versoes.iter().find(|v| u64::from(v.versao) == n),
            None => versoes.last(),
        };
        let Some(v) = alvo else {
            return erro(format!("não achei o padrão `{}`", cortar(regra, 60)));
        };
        let exemplos: Vec<String> = c
            .exemplos(&v.regra)
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.nome)
            .take(10)
            .collect();
        json!({
            "regra": v.regra, "versao": v.versao, "estado": estado_texto(v.estado),
            "motivo": cortar(&v.motivo, 120),
            "definicao": v.definicao,
            "script": v.script.as_deref().map(|s| cortar(s, MAX_SCRIPT_NA_LEITURA)),
            "exemplos": exemplos,
        })
        .to_string()
    }

    // ---- testar_regex ----

    fn testar_regex(&mut self, a: &Value) -> String {
        let Some(padrao) = a.get("regex").and_then(Value::as_str) else {
            return erro("falta `regex`");
        };
        let conserto = sem_barras_duplas(padrao);
        if conserto.is_some() {
            self.avisos.push(
                "o regex veio com barra invertida dupla (`\\\\s`): testei com uma só (`\\s`); mande a barra uma vez só".to_owned(),
            );
        }
        let padrao = conserto.as_deref().unwrap_or(padrao);
        let Some(texto) = self
            .ctx
            .tela
            .item
            .texto
            .as_deref()
            .filter(|t| !t.trim().is_empty())
        else {
            return erro("este item não tem texto guardado");
        };
        let r = match RegexBuilder::new(padrao)
            .size_limit(LIMITE_DO_REGEX)
            .dfa_size_limit(LIMITE_DO_REGEX)
            .build()
        {
            Ok(r) => r,
            Err(e) => return erro(format!("regex inválido: {}", cortar(&e.to_string(), 160))),
        };
        let grupos = r.captures_len() - 1;
        let grupo = match a.get("grupo").and_then(Value::as_u64) {
            Some(g) => usize::try_from(g).unwrap_or(usize::MAX),
            None => usize::from(grupos > 0),
        };
        if grupo > grupos {
            return erro(format!(
                "o regex tem {grupos} grupo(s) de captura e você pediu o grupo {grupo}"
            ));
        }
        let formato = match a.get("formato").and_then(Value::as_str) {
            Some(f) => match serde_json::from_value::<Formato>(json!(f)) {
                Ok(f) => Some(f),
                Err(_) => {
                    return erro(
                        "formato desconhecido: use texto, centavos, data, competencia, inteiro ou minusculas",
                    );
                }
            },
            None => None,
        };
        let momento = self.ctx.tela.item.momento.as_deref();
        let mut total = 0_usize;
        let mut itens = Vec::new();
        for caps in r.captures_iter(texto) {
            total += 1;
            if itens.len() >= MAX_CASAMENTOS {
                continue;
            }
            let Some(m) = caps.get(grupo) else { continue };
            let inicio = texto[..m.start()].rfind('\n').map_or(0, |i| i + 1);
            let fim = texto[m.start()..]
                .find('\n')
                .map_or(texto.len(), |i| m.start() + i);
            let mut item = json!({
                "linha": cortar(texto[inicio..fim].trim(), 90),
                "captura": cortar(m.as_str(), 60),
            });
            if let Some(f) = formato {
                match f.aplicar_em(m.as_str().trim(), momento) {
                    Some(v) => item["valor"] = json!(v.to_string()),
                    None => {
                        item["valor"] = Value::Null;
                        item["motivo"] = json!(cortar(
                            &f.motivo_da_falha("campo", m.as_str(), momento),
                            200
                        ));
                    }
                }
            }
            itens.push(item);
        }
        let mut saida = json!({"casamentos": total, "mostrados": itens.len(), "itens": itens});
        if total == 0 {
            let palavras: String = padrao
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { ' ' })
                .collect();
            let perto = linhas_parecidas(texto, &palavras, 3);
            saida["dica"] = json!(if perto.is_empty() {
                "o regex não casa com nada no texto: confira a grafia, use \\s* entre as palavras e não dependa de letras que o OCR troca".to_owned()
            } else {
                format!(
                    "o regex não casa com nada no texto. Linhas com palavras dele: {}",
                    perto
                        .iter()
                        .map(|l| format!("«{l}»"))
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            });
        }
        saida.to_string()
    }

    // ---- propor_padrao / pre_testar ----

    fn propor_padrao(&mut self, a: &Value) -> (String, bool) {
        match self.propor(a) {
            Ok(s) => (s, false),
            Err(Falha::Erro(m)) => (erro(m), false),
            Err(Falha::Repetida(m)) => (erro(m), true),
        }
    }

    /// O texto de um argumento, aparado e dentro do limite.
    fn texto_do_argumento(a: &Value, chave: &str, max: usize) -> Result<String, String> {
        let t = a
            .get(chave)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| format!("falta `{chave}`"))?;
        if t.chars().count() > max {
            return Err(format!("`{chave}` passa de {max} caracteres"));
        }
        Ok(t.to_owned())
    }

    /// A definição (e o script, se veio) dos argumentos, validada. Nada é gravado.
    fn ler_definicao(&mut self, a: &Value) -> Result<(Definicao, Option<String>), String> {
        let def_json = a.get("definicao").ok_or("falta `definicao`")?;
        // Alguns modelos mandam o objeto como texto.
        let def_json = match def_json {
            Value::String(s) => serde_json::from_str::<Value>(s)
                .or_else(|_| serde_json::from_str::<Value>(&reparar_escapes(s)))
                .map_err(|e| {
                    format!(
                        "`definicao` precisa ser um objeto JSON (não um texto): {}",
                        cortar(&e.to_string(), 120)
                    )
                })?,
            outro => outro.clone(),
        };
        let mut definicao: Definicao = serde_json::from_value(def_json)
            .map_err(|e| format!("definição inválida: {}", cortar(&e.to_string(), 240)))?;
        self.avisos.extend(consertar_barras(&mut definicao));
        definicao
            .validar()
            .map_err(|e| cortar(&e.to_string(), 240))?;
        let script = a
            .get("script")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|s| !s.trim().is_empty());
        if let Some(fonte) = &script {
            self.ctx
                .executor
                .verificar(fonte)
                .map_err(|e| format!("o script não compila: {}", cortar(&e.to_string(), 200)))?;
        }
        Ok((definicao, script))
    }

    /// Ensaia a definição sobre o item, sem gravar nada, e guarda a tentativa. Uma definição
    /// idêntica a uma tentativa anterior que não passou é recusada sem rodar.
    fn ensaiar(
        &mut self,
        regra: &str,
        nome: &str,
        definicao: &Definicao,
        script: Option<&str>,
    ) -> Result<(usize, PreTeste), Falha> {
        let chave = chave_da_definicao(definicao, script);
        if let Some(antes) = self
            .tentativas
            .iter()
            .find(|t| t.chave == chave && !t.pre_teste.passou)
        {
            let disse = if antes.pre_teste.item.diagnostico.is_empty() {
                antes.pre_teste.motivo.clone()
            } else {
                antes.pre_teste.item.diagnostico.join(" ")
            };
            let mude = if antes.pre_teste.item.faltam.is_empty() {
                "as condições de `detectar` que o diagnóstico aponta".to_owned()
            } else {
                format!(
                    "o regex ou o formato de {}",
                    antes.pre_teste.item.faltam.join(", ")
                )
            };
            return Err(Falha::Repetida(format!(
                "definição idêntica à tentativa {}; o pré-teste dela disse: {}; mude {mude} (ou use `testar_regex`) antes de tentar de novo",
                antes.numero,
                cortar(&disse, 400)
            )));
        }
        let c = self.catalogo().map_err(Falha::Erro)?;
        let existente = c
            .regras()
            .map_err(|_| Falha::Erro("falha ao ler os padrões".to_owned()))?
            .into_iter()
            .find(|r| r.id == regra);
        if let Some(r) = &existente
            && (r.nome != nome || r.tipo_documental != definicao.tipo_documental)
        {
            return Err(Falha::Erro(
                "o padrão já existe com outro nome ou tipo documental: use o mesmo, ou outro id"
                    .to_owned(),
            ));
        }
        let proxima = c
            .versoes(regra)
            .map(|v| u32::try_from(v.len()).unwrap_or(u32::MAX).saturating_add(1))
            .unwrap_or(1);
        let avulsa = VersaoDeRegra::avulsa(
            NovaVersao {
                regra: regra.to_owned(),
                nome: nome.to_owned(),
                definicao: definicao.clone(),
                script: script.map(str::to_owned),
                autor: Autor::Ia,
                motivo: self.pedido.clone(),
            },
            proxima,
            Estado::Rascunho,
        )
        .map_err(|e| Falha::Erro(cortar(&e.to_string(), 200)))?;
        let pre = pre_testar_versao(&self.ctx, &c, &avulsa, &self.pedido).map_err(Falha::Erro)?;
        let numero = self.tentativas.len() + 1;
        self.tentativas.push(Tentativa {
            numero,
            regra: regra.to_owned(),
            nome: nome.to_owned(),
            definicao: definicao.clone(),
            script: script.map(str::to_owned),
            chave,
            pre_teste: pre.clone(),
        });
        Ok((numero, pre))
    }

    /// A saída de um ensaio que não gravou nada.
    fn saida_do_ensaio(numero: usize, pre: &PreTeste, passou_por_propor: bool) -> String {
        let mut v = pre.para_o_modelo_json();
        if let Some(o) = v.as_object_mut() {
            o.remove("versao");
            o.insert("gravado".to_owned(), json!(false));
            o.insert("tentativa".to_owned(), json!(numero));
            o.insert(
                "aviso".to_owned(),
                json!(if pre.passou && !passou_por_propor {
                    "passou, mas NADA foi gravado (foi só um ensaio): chame `propor_padrao` com esta mesma definição para gravar o rascunho"
                } else {
                    "NADA foi gravado: este foi só um ensaio. Corrija o que o `diagnostico` aponta e chame `propor_padrao` de novo; uma definição idêntica é recusada"
                }),
            );
        }
        v.to_string()
    }

    fn propor(&mut self, a: &Value) -> Result<String, Falha> {
        let regra = Self::texto_do_argumento(a, "regra", 80).map_err(Falha::Erro)?;
        let nome = Self::texto_do_argumento(a, "nome", MAX_NOME).map_err(Falha::Erro)?;
        let (definicao, script) = self.ler_definicao(a).map_err(Falha::Erro)?;
        let (numero, pre) = self.ensaiar(&regra, &nome, &definicao, script.as_deref())?;
        if !pre.passou {
            return Ok(Self::saida_do_ensaio(numero, &pre, true));
        }
        self.gravar(&regra, &nome, definicao, script).map_err(|e| {
            self.gravacao_falhou = true;
            Falha::Erro(e)
        })
    }

    /// Grava a versão-rascunho (com o backup antes da primeira escrita e o rótulo de autoria),
    /// pré-testa a versão gravada e a põe no placar. Devolve a saída para o modelo.
    fn gravar(
        &mut self,
        regra: &str,
        nome: &str,
        definicao: Definicao,
        script: Option<String>,
    ) -> Result<String, String> {
        let caminho = self.ctx.catalogo_caminho();
        if !caminho.is_file() {
            return Err("não achei o catálogo de padrões do cofre".to_owned());
        }
        // Rede de segurança antes da primeira escrita da sessão (a mesma do MCP).
        self.backups.registrar(Qual::Regras, caminho.clone());
        self.backups.antes_de_escrever(Qual::Regras)?;
        let mut c =
            Catalogo::abrir(&caminho).map_err(|_| "falha ao abrir os padrões".to_owned())?;
        let existente = c
            .regras()
            .map_err(|_| "falha ao ler os padrões".to_owned())?
            .into_iter()
            .find(|r| r.id == regra);
        if let Some(r) = &existente
            && (r.nome != nome || r.tipo_documental != definicao.tipo_documental)
        {
            return Err(
                "o padrão já existe com outro nome ou tipo documental: use o mesmo, ou outro id"
                    .to_owned(),
            );
        }
        let antes = c.versoes(regra).map(|v| v.len()).unwrap_or(0);
        let versao = c
            .salvar_rascunho(NovaVersao {
                regra: regra.to_owned(),
                nome: nome.to_owned(),
                definicao,
                script,
                autor: Autor::Ia,
                motivo: self.pedido.clone(),
            })
            .map_err(|e| cortar(&e.to_string(), 200))?;
        let criou = c.versoes(regra).map(|v| v.len()).unwrap_or(0) > antes;
        if criou {
            // O rótulo do autor não é "se der": sem ele o rascunho fica sem dono visível. O erro
            // sobe para o modelo e para a tela.
            self.rotular(&mut c, regra, versao.versao)?;
        }
        let pre = pre_testar_versao(&self.ctx, &c, &versao, &self.pedido)?;
        let saida = pre.para_o_modelo();
        self.guardar(RascunhoCriado {
            regra: regra.to_owned(),
            versao: versao.versao,
            nome: nome.to_owned(),
            criou,
            neste_pedido: true,
            pre_teste: pre,
        });
        Ok(saida)
    }

    /// Grava `assistente (<modelo>)` na linha de criação do histórico da versão e confere que
    /// gravou.
    fn rotular(&self, c: &mut Catalogo, regra: &str, versao: u32) -> Result<(), String> {
        let rotulo = format!("assistente ({})", self.modelo);
        let falha = || {
            format!(
                "o rascunho `{}` v{versao} foi gravado, mas o rótulo de autoria não: confira o histórico dele antes de usar",
                cortar(regra, 60)
            )
        };
        c.rotular_autoria(regra, versao, &rotulo)
            .map_err(|_| falha())?;
        let gravado = c
            .historico_de_estado(regra, versao)
            .map_err(|_| falha())?
            .first()
            .is_some_and(|h| h.2 == rotulo);
        if gravado { Ok(()) } else { Err(falha()) }
    }

    fn guardar(&mut self, r: RascunhoCriado) {
        match self
            .rascunhos
            .iter_mut()
            .find(|x| x.regra == r.regra && x.versao == r.versao)
        {
            Some(x) => {
                let proposto = x.neste_pedido || r.neste_pedido;
                *x = RascunhoCriado {
                    neste_pedido: proposto,
                    ..r
                };
            }
            None => self.rascunhos.push(r),
        }
    }

    fn pre_testar(&mut self, a: &Value) -> String {
        if a.get("definicao").is_some_and(|d| !d.is_null()) {
            return match self.ensaio_inline(a) {
                Ok(s) => s,
                Err(Falha::Erro(m) | Falha::Repetida(m)) => erro(m),
            };
        }
        let Some(regra) = a.get("regra").and_then(Value::as_str) else {
            return erro("falta `regra` e `versao` (ou `definicao` para um ensaio)");
        };
        let Some(versao) = a
            .get("versao")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
        else {
            return erro("falta `versao` (número)");
        };
        let c = match self.catalogo() {
            Ok(c) => c,
            Err(e) => return erro(e),
        };
        match pre_testar(&self.ctx, &c, regra.trim(), versao) {
            Ok(pre) => {
                let saida = pre.para_o_modelo();
                let nome = c
                    .regras()
                    .ok()
                    .and_then(|rs| rs.into_iter().find(|r| r.id == regra.trim()))
                    .map_or_else(|| regra.to_owned(), |r| r.nome);
                self.guardar(RascunhoCriado {
                    regra: regra.trim().to_owned(),
                    versao,
                    nome,
                    criou: false,
                    neste_pedido: false,
                    pre_teste: pre,
                });
                saida
            }
            Err(e) => erro(e),
        }
    }

    /// `pre_testar` com a definição inline: um ensaio, sem gravar.
    fn ensaio_inline(&mut self, a: &Value) -> Result<String, Falha> {
        let (definicao, script) = self.ler_definicao(a).map_err(Falha::Erro)?;
        let regra = Self::texto_do_argumento(a, "regra", 80)
            .unwrap_or_else(|_| id_do_tipo(&definicao.tipo_documental));
        let nome = Self::texto_do_argumento(a, "nome", MAX_NOME)
            .unwrap_or_else(|_| cortar(&definicao.tipo_documental, MAX_NOME));
        let (numero, pre) = self.ensaiar(&regra, &nome, &definicao, script.as_deref())?;
        Ok(Self::saida_do_ensaio(numero, &pre, false))
    }

    // ---- titulos_da_lista ----

    fn titulos_da_lista(&self) -> String {
        let t = &self.ctx.tela;
        json!({
            "area": t.area,
            "filtro": t.filtro,
            "total": t.titulos.len(),
            "titulos": t.titulos.iter().take(MAX_TITULOS)
                .map(|x| json!({"id": cortar(&x.id, 16), "titulo": cortar(&x.titulo, 70)}))
                .collect::<Vec<_>>(),
        })
        .to_string()
    }
}

fn catalogo_do(ctx: &Contexto) -> Result<Catalogo, String> {
    let caminho = ctx.catalogo_caminho();
    if !caminho.is_file() {
        return Err("não achei o catálogo de padrões do cofre".to_owned());
    }
    Catalogo::abrir(&caminho).map_err(|_| "falha ao abrir os padrões".to_owned())
}

/// O que o motor leu do item e as versões em vigor (para o rastro e o seu resumo).
fn ler_o_item(
    ctx: &Contexto,
) -> Result<
    (
        sbf_regras::Leitura,
        Vec<sbf_regras::catalogo::VersaoDeRegra>,
    ),
    String,
> {
    let c = catalogo_do(ctx)?;
    let motor = sbf_regras::motor::Motor::das_vigentes(&c, Some(ctx.executor.clone()))
        .map_err(|_| "falha ao ler os padrões em vigor".to_owned())?;
    let vigentes = c.vigentes().unwrap_or_default();
    Ok((motor.ler(&ctx.tela.item), vigentes))
}

fn prioridade_e_especificidade(
    vigentes: &[sbf_regras::catalogo::VersaoDeRegra],
    regra: &str,
    versao: u32,
) -> Value {
    vigentes
        .iter()
        .find(|v| v.regra == regra && v.versao == versao)
        .map_or(json!({}), |v| {
            json!({
                "prioridade": v.definicao.prioridade,
                "especificidade": v.definicao.especificidade(),
            })
        })
}

/// O rastro completo do item (a ferramenta `rastro_do_item`).
fn rastro_do_item(ctx: &Contexto) -> Result<Value, String> {
    let (leitura, vigentes) = ler_o_item(ctx)?;
    let info = |regra: &str, versao: u32| prioridade_e_especificidade(&vigentes, regra, versao);
    let decisao = match &leitura.decisao {
        sbf_regras::Decisao::Reconhecido(r) => json!({
            "decisao": "reconhecido",
            "vencedor": {"regra": r.regra, "versao": r.versao, "tipo": r.tipo_documental,
                "info": info(&r.regra, r.versao), "motivos": r.motivos.iter().take(4).collect::<Vec<_>>()},
        }),
        sbf_regras::Decisao::Ambiguo { candidatas, motivo } => json!({
            "decisao": "ambiguo",
            "motivo": cortar(motivo, 200),
            "candidatas": candidatas.iter().map(|r| json!({
                "regra": r.regra, "versao": r.versao, "tipo": r.tipo_documental,
                "info": info(&r.regra, r.versao),
                "campos": r.campos.iter().map(|(n, c)| (n.clone(), cortar(&c.valor.to_string(), 40))).collect::<std::collections::BTreeMap<_, _>>(),
            })).collect::<Vec<_>>(),
        }),
        sbf_regras::Decisao::Nenhuma => json!({"decisao": "nenhuma"}),
    };
    Ok(json!({
        "tipo_de_entrada": ctx.tela.item.tipo,
        "avaliados": leitura.execucoes.iter().map(|e| json!({
            "regra": e.regra, "versao": e.versao, "resultado": e.resultado,
        })).collect::<Vec<_>>(),
        "resultado": decisao,
    }))
}

/// Quantos padrões que casaram o resumo lista.
const MAX_CASARAM_NO_RESUMO: usize = 5;

/// O resumo curto do rastro do item para o primeiro prompt (D8): quantos padrões estão em
/// vigor, quais casaram (com prioridade e especificidade) e o que o motor decidiu. Poupa a
/// chamada de `rastro_do_item`. `None` quando o cofre não tem catálogo de padrões.
pub fn rastro_resumido(ctx: &Contexto) -> Option<String> {
    let (leitura, vigentes) = ler_o_item(ctx).ok()?;
    let casaram: Vec<String> = leitura
        .execucoes
        .iter()
        .filter(|e| e.resultado == sbf_regras::ResultadoDaExecucao::Reconhecido)
        .map(|e| {
            let i = prioridade_e_especificidade(&vigentes, &e.regra, e.versao);
            format!(
                "`{}` v{} (prioridade {}, especificidade {})",
                e.regra, e.versao, i["prioridade"], i["especificidade"]
            )
        })
        .collect();
    let mut linhas = vec![format!("padrões em vigor: {}", vigentes.len())];
    linhas.push(if casaram.is_empty() {
        "casaram neste item: nenhum".to_owned()
    } else {
        let extra = casaram.len().saturating_sub(MAX_CASARAM_NO_RESUMO);
        format!(
            "casaram neste item: {}{}",
            casaram
                .iter()
                .take(MAX_CASARAM_NO_RESUMO)
                .cloned()
                .collect::<Vec<_>>()
                .join("; "),
            if extra > 0 {
                format!(" (e mais {extra})")
            } else {
                String::new()
            }
        )
    });
    linhas.push(match &leitura.decisao {
        sbf_regras::Decisao::Reconhecido(r) => {
            format!(
                "decisão do motor: reconhecido; vence `{}` v{}",
                r.regra, r.versao
            )
        }
        sbf_regras::Decisao::Ambiguo { motivo, candidatas } => format!(
            "decisão do motor: AMBÍGUO entre {}; {}",
            candidatas
                .iter()
                .map(|r| format!("`{}` v{}", r.regra, r.versao))
                .collect::<Vec<_>>()
                .join(" e "),
            cortar(motivo, 200)
        ),
        sbf_regras::Decisao::Nenhuma => "decisão do motor: nenhum padrão reconhece".to_owned(),
    });
    Some(linhas.join("\n"))
}

fn estado_texto(e: Estado) -> &'static str {
    match e {
        Estado::Rascunho => "rascunho",
        Estado::Ativa => "ativa",
        Estado::Consolidada => "consolidada",
        Estado::Desativada => "desativada",
    }
}

fn condicoes_curtas(d: &Definicao) -> String {
    let uma = |c: &Condicao| -> String {
        match c {
            Condicao::Contem(t) => format!("contem:{}", cortar(t, 30)),
            Condicao::Regex(p) => format!("regex:{}", cortar(p, 30)),
            Condicao::Mime(m) => format!("mime:{}", m.join("|")),
            Condicao::Canal(_) => "canal".to_owned(),
            Condicao::Descricao(p) => format!("descricao:{}", cortar(p, 30)),
            Condicao::Nome(p) => format!("nome:{}", cortar(p, 30)),
            Condicao::ContraparteDoc(_) => "contraparte_doc".to_owned(),
            Condicao::Contraparte(p) => format!("contraparte:{}", cortar(p, 30)),
        }
    };
    let mut partes: Vec<String> = d.detectar.todos.iter().map(&uma).collect();
    if !d.detectar.algum.is_empty() {
        partes.push(format!(
            "algum({})",
            d.detectar
                .algum
                .iter()
                .map(&uma)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if !d.detectar.nenhum.is_empty() {
        partes.push(format!(
            "nenhum({})",
            d.detectar
                .nenhum
                .iter()
                .map(&uma)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    cortar(&partes.join(" e "), 160)
}
