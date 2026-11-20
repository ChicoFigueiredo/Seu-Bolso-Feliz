//! As ferramentas de Rita (spec 15, "Ferramentas de Rita"): poucas, de saída curta, e nenhuma
//! que ative, consolide, reverta, vincule, concilie ou escreva fora do `regras.sqlite`.
//!
//! Quem impõe o teto de chamadas é a [`Sessao`], não o modelo: a chamada de número 9 é recusada
//! com texto fixo, venha de onde vier (laço próprio ou servidor MCP do `claude -p`). Uma
//! ferramenta fora da lista também é recusada (e conta).

use sbf_mcp::backup::{Backups, Qual};
use sbf_regras::catalogo::{Catalogo, NovaVersao};
use sbf_regras::definicao::{Condicao, Definicao};
use sbf_regras::{Autor, Estado};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::contexto::Contexto;
use crate::pre_teste::{PreTeste, pre_testar};
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

/// Os nomes das ferramentas de Rita, na ordem em que aparecem para o modelo.
pub const FERRAMENTAS: [&str; 7] = [
    "ver_item",
    "rastro_do_item",
    "listar_padroes",
    "ver_padrao",
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
            "propor_padrao",
            "Grava um RASCUNHO de padrão (nunca ativa) e já devolve o pré-teste dele. Se `passou` vier false, corrija e chame de novo com o mesmo `regra`.",
            json!({"type": "object", "properties": {
                "regra": {"type": "string", "description": "Id: minúsculas, dígitos e hífen."},
                "nome": {"type": "string", "description": "Nome para mostrar (até 120 caracteres)."},
                "definicao": {"type": "object", "description": "A definição no formato das instruções: tipo_documental, aplica_a, prioridade, detectar{todos,algum,nenhum}, extrair{campo:{regex,grupo,formato}}, obrigatorios, propor."},
                "script": {"type": "string", "description": "Script Luau opcional; só para o que a definição não resolve."}
            }, "required": ["regra", "nome", "definicao"]}),
        ),
        def(
            "pre_testar",
            "Pré-testa uma versão já gravada: o item, os exemplos e os documentos parecidos do cofre. Só leitura.",
            json!({"type": "object", "properties": {
                "regra": {"type": "string"},
                "versao": {"type": "integer"}
            }, "required": ["regra", "versao"]}),
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
        }
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
        let (saida, recusada) = match nome {
            "ver_item" => (self.ver_item(argumentos), false),
            "rastro_do_item" => (self.rastro_do_item(), false),
            "listar_padroes" => (self.listar_padroes(argumentos), false),
            "ver_padrao" => (self.ver_padrao(argumentos), false),
            "propor_padrao" => (self.propor_padrao(argumentos), false),
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
        self.registrar(numero, nome, argumentos, &saida, recusada);
        saida
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

    // ---- propor_padrao / pre_testar ----

    fn propor_padrao(&mut self, a: &Value) -> String {
        match self.propor(a) {
            Ok(s) | Err(s) => s,
        }
    }

    fn propor(&mut self, a: &Value) -> Result<String, String> {
        let texto = |chave: &str, max: usize| -> Result<String, String> {
            let t = a
                .get(chave)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .ok_or_else(|| erro(format!("falta `{chave}`")))?;
            if t.chars().count() > max {
                return Err(erro(format!("`{chave}` passa de {max} caracteres")));
            }
            Ok(t.to_owned())
        };
        let regra = texto("regra", 80)?;
        let nome = texto("nome", MAX_NOME)?;
        let def_json = a
            .get("definicao")
            .ok_or_else(|| erro("falta `definicao`"))?;
        // Alguns modelos mandam o objeto como texto.
        let def_json = match def_json {
            Value::String(s) => serde_json::from_str::<Value>(s)
                .map_err(|_| erro("`definicao` precisa ser um objeto JSON"))?,
            outro => outro.clone(),
        };
        let definicao: Definicao = serde_json::from_value(def_json).map_err(|e| {
            erro(format!(
                "definição inválida: {}",
                cortar(&e.to_string(), 240)
            ))
        })?;
        definicao
            .validar()
            .map_err(|e| erro(cortar(&e.to_string(), 240)))?;
        let script = a
            .get("script")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|s| !s.trim().is_empty());
        if let Some(fonte) = &script {
            self.ctx.executor.verificar(fonte).map_err(|e| {
                erro(format!(
                    "o script não compila: {}",
                    cortar(&e.to_string(), 200)
                ))
            })?;
        }
        let caminho = self.ctx.catalogo_caminho();
        if !caminho.is_file() {
            return Err(erro("não achei o catálogo de padrões do cofre"));
        }
        // Rede de segurança antes da primeira escrita da sessão (a mesma do MCP).
        self.backups.registrar(Qual::Regras, caminho.clone());
        self.backups.antes_de_escrever(Qual::Regras).map_err(erro)?;
        let mut c = Catalogo::abrir(&caminho).map_err(|_| erro("falha ao abrir os padrões"))?;
        let existente = c
            .regras()
            .map_err(|_| erro("falha ao ler os padrões"))?
            .into_iter()
            .find(|r| r.id == regra);
        if let Some(r) = &existente
            && (r.nome != nome || r.tipo_documental != definicao.tipo_documental)
        {
            return Err(erro(
                "o padrão já existe com outro nome ou tipo documental: use o mesmo, ou outro id",
            ));
        }
        let antes = c.versoes(&regra).map(|v| v.len()).unwrap_or(0);
        let versao = c
            .salvar_rascunho(NovaVersao {
                regra: regra.clone(),
                nome: nome.clone(),
                definicao,
                script,
                autor: Autor::Ia,
                motivo: self.pedido.clone(),
            })
            .map_err(|e| erro(cortar(&e.to_string(), 200)))?;
        let criou = c.versoes(&regra).map(|v| v.len()).unwrap_or(0) > antes;
        if criou {
            // O rótulo do autor não é "se der": sem ele o rascunho fica sem dono visível. O erro
            // sobe para o modelo e para a tela.
            self.rotular(&mut c, &regra, versao.versao)?;
        }
        let pre = pre_testar(&self.ctx, &c, &regra, versao.versao).map_err(erro)?;
        let saida = pre.para_o_modelo();
        self.guardar(RascunhoCriado {
            regra,
            versao: versao.versao,
            nome,
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
            erro(format!(
                "o rascunho `{}` v{versao} foi gravado, mas o rótulo de autoria não: confira o histórico dele antes de usar",
                cortar(regra, 60)
            ))
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
        let Some(regra) = a.get("regra").and_then(Value::as_str) else {
            return erro("falta `regra`");
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
