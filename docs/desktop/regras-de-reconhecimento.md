# Regras de reconhecimento: como criar uma sem recompilar o sbf

Este guia é para o dono do cofre. Ele ensina a criar, testar e ativar uma **regra de
reconhecimento** (termo do `CONTEXT.md`): a instrução que diz como reconhecer um tipo de
documento ou registro, quais campos tirar dele e o que propor ao usuário. A regra é dado
versionado no `regras.sqlite` do cofre. Mudá-la não exige compilar nada.

Exemplo de tipo que se cadastra assim: um comprovante de Pix de um banco fictício,
"Comprovante Pix · Banco Exemplo".

## 1. O ciclo de uma regra

| Estado          | O que faz                                                                                     | Quem leva a ele                                                                            |
| --------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| **Rascunho**    | Nunca roda na caixa de entrada. Só serve para teste e simulação.                              | Qualquer um que salva (dono, IA, correção)                                                 |
| **Ativa**       | Reconhece e **propõe**. Nada vira conciliação sem o clique do usuário.                        | Só o usuário (janela ou terminal interativo)                                               |
| **Consolidada** | Como a ativa, e concilia sozinha quando as três provas batem exatamente. Sempre com Desfazer. | O sbf sugere depois de **5 propostas aceitas seguidas**, sem correção; só o usuário aprova |
| **Desativada**  | Saiu de cena e fica no histórico.                                                             | Desativar, ou a substituição por uma versão nova ativada                                   |

Regras do ciclo:

- Uma versão **nunca muda depois de salva**. Corrigir é salvar outra versão.
- Só uma versão por regra fica ativa ou consolidada.
- Uma versão nova **nasce rascunho**, mesmo quando veio da IA. Para entrar em vigor, precisa
  ser ativada pelo usuário.
- Ativar só funciona se **todos os exemplos passam**. Um exemplo que falha bloqueia a ativação.
- Uma rejeição, um Desfazer ou uma versão nova devolve a regra consolidada para ativa.
- **Rollback**: `sbf regras ... reverter <regra>` reativa a versão anterior, sem apagar nada.
  Ele não roda os exemplos de novo, porque a versão já esteve em vigor.

Quem pode mudar o estado: o usuário, na janela do sbf ou num terminal interativo que
confirmou, e o MCP (D15, 2026-11-18: controle total, com `mcp` no histórico de estado). Os
exemplos que falham bloqueiam a ativação para todos. Uma IA sem essa decisão (`Quem::Ia`)
segue sem ativar, consolidar, desativar nem reverter (ADR-021).

## 2. A definição (formato JSON)

Uma regra é um arquivo JSON com três campos: `regra` (o id), `nome` (para mostrar) e
`definicao` (o formato declarativo). O id usa só minúsculas, dígitos e hífen.

```json
{
  "regra": "comprovante-pix-banco-exemplo",
  "nome": "Comprovante Pix · Banco Exemplo",
  "definicao": { "...": "..." }
}
```

Esse arquivo é o que `sbf regras ... importar` lê. O MCP (`propor_regra`) recebe a `definicao`
sozinha, com `regra`, `nome` e `motivo` como campos separados. Dentro de `definicao`, os
campos são:

| Campo               | Tipo          | Padrão          | O que faz                                                                           |
| ------------------- | ------------- | --------------- | ----------------------------------------------------------------------------------- |
| `versao_do_esquema` | número        | 1               | Sempre `1` por enquanto. Outro valor é recusado.                                    |
| `tipo_documental`   | texto         | obrigatório     | O tipo que a regra reconhece (ex.: `comprovante-pix-banco-exemplo`).                |
| `aplica_a`          | lista         | `["documento"]` | Tipos de entrada: `documento`, `registro` ou `mensagem`.                            |
| `prioridade`        | inteiro       | 0               | Vence a de maior prioridade entre as que casam.                                     |
| `detectar`          | objeto        | obrigatório     | As condições de reconhecimento: `todos`, `algum`, `nenhum` (ver abaixo).            |
| `extrair`           | mapa          | `{}`            | Campo a campo: nome → extração (ver abaixo).                                        |
| `obrigatorios`      | lista         | `[]`            | Campos que precisam aparecer para a regra reconhecer. Senão, a regra não se aplica. |
| `leitor`            | texto ou nulo | nulo            | Um leitor nativo do sbf que roda antes do script (hoje: `contracheque`).            |
| `parametros`        | lista         | `[]`            | Nomes dos parâmetros versionados que o script recebe (ex.: `irrf`).                 |
| `script_le_texto`   | booleano      | falso           | Se `true`, o script recebe o texto do documento.                                    |
| `texto_de_ocr`      | booleano      | falso           | Se `true`, o texto vem de OCR (imagem) e é limpo do ruído antes de ler (ver 2.6).   |
| `propor`            | lista         | `[]`            | As propostas que a regra faz (ver abaixo).                                          |

### 2.1 Condições de `detectar`

Uma condição casa quando: **todas** de `todos` são verdadeiras, **ao menos uma** de `algum`
é verdadeira (se a lista não estiver vazia) e **nenhuma** de `nenhum` é verdadeira. Precisa
haver ao menos uma condição em `todos` ou em `algum`.

| Forma                           | Exemplo                                   | Casa quando                                                                                                    |
| ------------------------------- | ----------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `{"contem": "trecho"}`          | `{"contem": "comprovante pix"}`           | O texto normalizado (minúsculas, sem acento, espaços juntos) contém o trecho.                                  |
| `{"regex": "..."}`              | `{"regex": "Chave:\\s*\\S+"}`             | Uma expressão regular casa o texto original.                                                                   |
| `{"mime": ["application/pdf"]}` | `{"mime": ["image/*"]}`                   | O tipo real do conteúdo é um destes (pela assinatura, não pela extensão).                                      |
| `{"canal": ["pluggy"]}`         | `{"canal": ["gmail", "telegram"]}`        | O canal da origem é um destes: `arrastado`, `pasta`, `cofre`, `gmail`, `telegram`, `pluggy`, `mcp`, `manual`.  |
| `{"descricao": "..."}`          | `{"descricao": "PIX ENVIADO"}`            | Regex sobre a descrição que a origem já trouxe (ex.: a transação da Pluggy).                                   |
| `{"nome": "..."}`               | `{"nome": "^comprovante_"}`               | Regex sobre o nome do arquivo.                                                                                 |
| `{"contraparte_doc": [...]}`    | `{"contraparte_doc": ["000.000.000-00"]}` | O CPF/CNPJ do favorecido do registro é igual a algum da lista. Só os dígitos contam (ver 2.5).                 |
| `{"contraparte": "..."}`        | `{"contraparte": "^fulana de tal$"}`      | Regex sobre o nome do favorecido do registro, **normalizado** (minúsculas, sem acento), sem diferenciar caixa. |

### 2.2 Extração (`extrair`)

Cada campo tem uma regex, um grupo (padrão `1`: o primeiro parêntese), um formato e uma fonte:

```json
"valor_centavos": { "regex": "Valor:\\s*R\\$\\s*([\\d.,]+)", "formato": "centavos" }
```

| `formato`     | O que vira                                                   |
| ------------- | ------------------------------------------------------------ |
| `texto`       | O trecho como veio.                                          |
| `centavos`    | Dinheiro em centavos inteiros (`R$ 1.234,56` vira `123456`). |
| `data`        | `AAAA-MM-DD`, a partir de `DD/MM/AAAA` e similares.          |
| `competencia` | `AAAA-MM`.                                                   |
| `inteiro`     | Número inteiro.                                              |
| `minusculas`  | Texto em minúsculas (para constantes como `pix`).            |

`fonte` é `texto` (padrão: o texto do documento), `descricao` (a descrição que a origem já
trouxe, como a da Pluggy), `contraparte` (o nome do favorecido do registro) ou
`contraparte_doc` (o CPF/CNPJ dele). O campo extraído de uma `contraparte_doc` nunca fica
gravado na proposta (ver 2.5). Dinheiro é sempre em centavos; nunca use ponto flutuante.

Os nomes dos campos que o sbf conhece com significado próprio são `valor_centavos`, `data`,
`vencimento`, `conta`, `contrato`, `autenticacao` e `cartao`. Se duas regras empatadas
discordam num deles, o sbf não escolhe sozinho: a entrada vai para revisão.

### 2.3 Propostas (`propor`)

A regra **propõe**; ela não grava. Cada proposta é uma destas formas:

| Forma                                                                                                          | Use para                                                                                                                                                                                                 |
| -------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `{"provar_registro": {"janela_dias": 2}}`                                                                      | O documento prova um registro do banco que já existe (o comprovante prova o débito). Nunca cria gasto novo.                                                                                              |
| `{"conciliar_ocorrencia": {"janela_dias": 3, "compromisso": "Energia"}}`                                       | Ligar a ocorrência de um compromisso do mês ao documento. `compromisso` é opcional.                                                                                                                      |
| `"pagar_fatura"`                                                                                               | O débito no banco paga uma fatura de cartão. É pagamento, não gasto (os itens da fatura já são os gastos).                                                                                               |
| `{"vincular_ocorrencia": {"compromisso": "servico exemplo", "tipo_documento": "comprovante", "janela_dias": 20}}` | Anexar o documento à ocorrência do mês do compromisso mais perto da data dele, sem exigir valor nem débito no extrato (cobrança em dólar no cartão). Empate de distância não sugere; o usuário confirma. |

Essas propostas usam os campos `valor_centavos`, `data` (ou `vencimento`, na fatura), `conta`
e `cartao`; `vincular_ocorrencia` só pede `data` (ou `vencimento`). Os formatos `data` e
`competencia` leem também datas em inglês (`November 20, 2026`, `Nov 20, 2026`, `20 November 2026`). Sem o campo necessário, a proposta não sai e a regra diz o que falta.

### 2.4 Exemplo completo: Comprovante Pix · Banco Exemplo (fictício)

Arquivo `comprovante-pix-banco-exemplo.json`:

```json
{
  "regra": "comprovante-pix-banco-exemplo",
  "nome": "Comprovante Pix · Banco Exemplo",
  "definicao": {
    "versao_do_esquema": 1,
    "tipo_documental": "comprovante-pix-banco-exemplo",
    "aplica_a": ["documento"],
    "prioridade": 0,
    "detectar": {
      "todos": [{ "contem": "comprovante pix" }, { "contem": "banco exemplo" }]
    },
    "extrair": {
      "valor_centavos": {
        "regex": "Valor:\\s*R\\$\\s*([\\d.,]+)",
        "formato": "centavos"
      },
      "data": {
        "regex": "Data:\\s*(\\d{2}/\\d{2}/\\d{4})",
        "formato": "data"
      },
      "autenticacao": {
        "regex": "Autentica\\S+:\\s*(\\S+)",
        "formato": "texto"
      }
    },
    "obrigatorios": ["valor_centavos"],
    "propor": [{ "provar_registro": { "janela_dias": 2 } }]
  }
}
```

Um texto de exemplo que essa regra reconhece (dados fictícios), salvo como `comprovante.txt`:

```text
Comprovante Pix
Banco Exemplo
Valor: R$ 150,00
Data: 10/11/2026
Autenticação: ABC123XYZ
```

Ela extrai `valor_centavos = 15000`, `data = 2026-11-10` e `autenticacao = ABC123XYZ`, e
propõe provar um registro com até 2 dias de folga em volta da data.

### 2.5 O favorecido de um Pix: o exemplo da diarista (fictício)

Na CAIXA a descrição de um Pix é só "PIX ENVIADO". O favorecido vem em colunas à parte do
registro da Pluggy no inventário: `contraparte` (o nome) e `contraparte_doc` (o CPF/CNPJ, às
vezes formatado). Uma regra com `aplica_a: ["registro"]` reconhece o favorecido por elas e
propõe conciliar o compromisso mensal que ele recebe. A ocorrência do compromisso nomeado em
`compromisso` ("Diarista") junta os débitos que a regra reconhece:

- **Janela**: a competência inteira da ocorrência, mais a folga `janela_dias` (no máximo 60)
  em volta do vencimento.
- **Um débito** com o valor previsto: sugestão de um. **Vários** que somam o previsto (todos,
  ou o menor subconjunto de até 6): sugestão combinada, "Pix para Fulana de Tal · 2 débitos
  somam o previsto". Senão, todos os reconhecidos, com o aviso "⚠ soma R$ X difere do
  previsto R$ Y".
- Débito já conciliado a outra ocorrência, ou de uma proposta que você já decidiu, não entra.
- **Confirmar** grava uma conciliação por débito, todas na mesma ocorrência, e o "Conciliado"
  soma; cada proposta recebe `Aceito`. **Ignorar** desfaz as sugestões e registra `Rejeitado`.
  Nada muda sem o clique. Só uma regra **consolidada**, com soma exata e a conta certa (as
  três provas), concilia sozinha, com Desfazer.
- As sugestões aparecem no Controle mensal (selo "conciliar?" e a seção Movimentos do
  Detalhe) depois de "Buscar no extrato", e o MCP `listar_compromissos` mostra
  `sugestao_de_conciliacao: true`. Regra em rascunho nunca sugere.

Arquivo `pix-da-diarista.json` (documento fictício):

```json
{
  "regra": "pix-da-diarista",
  "nome": "Pix · Diarista",
  "definicao": {
    "versao_do_esquema": 1,
    "tipo_documental": "pix-da-diarista",
    "aplica_a": ["registro"],
    "detectar": {
      "todos": [{ "contraparte_doc": ["000.000.000-00"] }]
    },
    "extrair": {
      "favorecido": {
        "regex": "(.+)",
        "formato": "texto",
        "fonte": "contraparte"
      }
    },
    "propor": [{ "conciliar_ocorrencia": { "janela_dias": 5, "compromisso": "Diarista" } }]
  }
}
```

Cuidados com o documento: a comparação ignora a pontuação dos dois lados (`000.000.000-00`
casa `00000000000`) e cada documento da lista precisa ter 11 (CPF) ou 14 (CNPJ) dígitos. O
documento nunca vai para a tabela `execucao` nem para a proposta gravada (o catálogo tira o
campo cuja origem é `contraparte_doc`) e nunca vai a log. O script só recebe os campos já
extraídos: se a regra não extrai o documento, o script não o vê.

### 2.6 Texto de imagem (OCR): `"texto_de_ocr": true`

Prints de app e fotos de comprovante (PNG, JPG, JPEG, WEBP, até 15 MiB) passam pelo OCR do
`tesseract` (idioma `por`) quando o sbf extrai o texto, tanto ao reconhecer um arquivo solto
quanto no worker, que lê as imagens novas do inventário. O texto fica em `texto` com o método
`ocr-tesseract`. Sem o `tesseract` instalado a imagem fica "sem texto (OCR indisponível)": não é
erro, nada é gravado, e o OCR roda quando ele for instalado.

Texto de OCR vem com ruído, e uma regra que lê imagem pode pedir a limpeza:

```json
"definicao": { "texto_de_ocr": true, "...": "..." }
```

Com a opção ligada, o motor passa o texto por esta normalização **antes** de detectar, extrair
e entregar ao script (quem não liga continua lendo o texto como saiu do extrator):

| O que o OCR erra                                  | O que a normalização faz                                                                                                                                                                                                                                                              |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Espaços e tabs repetidos, espaço no fim da linha  | Junta em um espaço e apara o fim de cada linha (as quebras de linha ficam).                                                                                                                                                                                                           |
| `O` (letra) e `l` (L minúsculo) no meio de número | `O` vira `0` e `l` vira `1`, **só dentro de um número**: um token só de dígitos, `.`, `,`, `/`, `:`, `-`, `O` e `l`, com ao menos um dígito de verdade (`2O26` vira `2026`, `l.234,5O` vira `1.234,50`). Palavras e tokens com outras letras (`Pol1`, `Olá`, `BOL2`) não são tocados. |
| `RS` ou `R§` no lugar de `R$`                     | Vira `R$` quando vem antes de um valor com vírgula e dois decimais (`RS 1.234,56`). O `RS` de Rio Grande do Sul antes de um CEP fica.                                                                                                                                                 |

A normalização é idempotente e só vale para a regra que pede. A opção **não aparece no JSON
quando é falsa**, então as regras que já existem mantêm o mesmo checksum. As posições dos campos
extraídos (`de_onde`) valem para o texto já limpo, não para o que o tesseract devolveu. Escreva o
regex da regra pensando no texto limpo: `Valor:\s*R\$\s*([\d.,]+)` lê `Valor:  RS 1.234,5O`.

## 3. Exemplos: o teste que libera a ativação

Cada regra tem exemplos. Um exemplo diz o que o motor deve fazer com um documento de
texto: reconhecer com certos campos, ou não se aplicar. A regra só ativa se todos passam.

- O arquivo do exemplo é um `.txt` com o texto já extraído. PDF ainda não é lido por este
  comando.
- O resultado esperado é um JSON (o formato de `Esperado`):

```json
{
  "esperado": "reconhece",
  "tipo_documental": "comprovante-pix-banco-exemplo",
  "campos": { "valor_centavos": { "tipo": "centavos", "valor": 15000 } }
}
```

ou, para um texto que a regra **não** deve reconhecer:

```json
{ "esperado": "nao_se_aplica" }
```

Na conferência, campos a mais no resultado não reprovam; só os listados no esperado são
comparados.

## 4. Scripts (Luau)

Um **script** (termo do `CONTEXT.md`) é um trecho curto em Luau, anexado a uma regra, para
calcular ou transformar campos que a parte declarativa não alcança. Ele só lê o que recebe e
devolve uma proposta. Nunca grava, paga, concilia nem acessa arquivo, rede ou banco.

**O que entra.** O script define a função global `transformar(entrada, parametros, ajuda)`:

- `entrada`: tabela somente leitura com `tipo_documental`, `canal` e `campos` (os campos já
  extraídos: centavos e inteiros como números; datas e competências como texto `AAAA-MM-DD`
  e `AAAA-MM`; booleanos). `texto` só existe quando `script_le_texto` é `true`.
- `parametros`: tabela somente leitura com os parâmetros versionados listados em
  `parametros`, cada um na versão vigente na competência da entrada (`parametros.irrf`).
- `ajuda`: funções puras do host: `ajuda.mul_div_arred(a, b, c)` (a·b/c em inteiros,
  arredondado metade para longe do zero), `ajuda.centavos("1.234,56")`,
  `ajuda.data("06/10/2026")` e `ajuda.normalizar(texto)`.

**O que volta.** Uma tabela com:

- `corresponde` (booleano, obrigatório);
- `tipo_documental` (texto, opcional);
- `campos` (nome → valor). Nomes que terminam em `_centavos` têm de ser inteiros e viram
  centavos; nomes que começam por `data` ou `vencimento` viram data (`AAAA-MM-DD`); inteiros
  viram inteiro; o resto vira texto;
- `revisar` (booleano), `motivos` e `evidencias` (listas de texto);
- `exibir` (lista de textos, opcional): linhas curtas que o script quer mostrar no cartão do
  Reconhecimento, abaixo da linha "Regra <nome> · versão N". No máximo 5 linhas, cada uma
  com até 160 caracteres, só texto; fora disso é retorno inválido. Ausente vale lista vazia,
  então scripts antigos seguem válidos. É só para a tela: não entra na chave de
  idempotência, não vai para a tabela `execucao` nem para a proposta gravada, porque pode
  conter texto do documento. Linhas que começam por `⚠` saem na cor de aviso.

Qualquer outra forma de retorno é erro, e a entrada vai para revisão sem travar a fila.

**O que não existe.** `io`, `os`, `package`, `require`, `debug`, `load`/`loadstring`,
`dofile`/`loadfile`, `getfenv`/`setfenv`, `collectgarbage`, `math.random` e relógio. Não há
rede nem acesso ao banco. Os limites (memória, tempo, número de instruções, tamanho da
entrada e da saída) são aplicados por execução. O script roda num processo filho.

Exemplo mínimo (fictício):

```lua
function transformar(entrada, parametros, ajuda)
  local valor = entrada.campos.valor_centavos
  if valor == nil then
    return { corresponde = false }
  end
  return {
    corresponde = true,
    campos = { taxa_centavos = ajuda.mul_div_arred(valor, 15, 1000) },
    evidencias = { "taxa-fixa-15-por-mil" },
  }
end
```

Um script se confere antes de salvar (`importar --script` e `propor_regra`): sintaxe e a
existência de `transformar`. Ativar exige a verificação de novo.

## 5. Parâmetros versionados: o exemplo do IRRF 2026

Uma tabela oficial (faixas, alíquotas, dedução) não cabe no script. Ela vira um **parâmetro
versionado**, vigente por competência e imutável depois de gravado. Cada versão tem:

- `nome` (ex.: `irrf`), `versao` (o sbf numera);
- `vigencia_inicio` e `vigencia_fim` (`AAAA-MM`; sem fim, a vigência é aberta);
- `valor` (o JSON da tabela);
- `fonte` (de onde veio: a norma ou a URL oficial), para a conferência.

Um script que pede `"irrf"` na lista `parametros` recebe a versão que vale na competência do
documento. Uma correção de tabela é uma **versão nova do parâmetro**, não uma edição.

O arquivo abaixo tem valores **fictícios**, só para mostrar o formato. Os valores reais vêm
da tabela oficial de 2026, copiada e conferida na fonte antes de gravar:

```json
{
  "faixas": [
    { "teto_cents": 100000, "aliquota_ppm": 0, "parcela_deduzir_cents": 0 },
    { "teto_cents": 200000, "aliquota_ppm": 75000, "parcela_deduzir_cents": 15000 }
  ]
}
```

```text
sbf regras --inventario <cofre> parametro irrf 2026-01 irrf-2026.json \
  --fonte "<url oficial da tabela>"
```

O IRRF é só **conferência**: o script calcula a estimativa e mostra um alerta quando ela
diverge do imposto retido no contracheque. Nunca sobrescreve a rubrica do documento. No
cartão do contracheque ele diz o que concluiu numa linha (`exibir`): "IRRF conferido ✓ ·
tabela <versão>", a mesma com "(1 centavo de arredondamento)", "⚠ IRRF diverge da tabela
<versão>" (o aviso com os valores continua vindo do sbf) ou "IRRF não conferido: <motivo>".
Como o script de fábrica mudou, o sbf grava uma versão nova (rascunho) da regra
`contracheque` ao abrir; quem ativa é você.

## 6. Comandos de terminal

Todos rodam como `sbf regras --inventario <cofre> <subcomando>`. O catálogo é
`<cofre>/regras.sqlite`.

| Subcomando                                                                                     | O que faz                                                        |
| ---------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `listar`                                                                                       | As regras, o que está em vigor e as sugestões de consolidação.   |
| `mostrar <regra>`                                                                              | Versões, exemplos e histórico de estado.                         |
| `importar <arquivo.json> [--script <arquivo.luau>] [--motivo <texto>]`                         | Cria um **rascunho**. Nunca ativa.                               |
| `exemplo <regra> <arquivo.txt> (--nao-se-aplica \| --esperado <arquivo.json>) [--nome <nome>]` | Grava um exemplo de teste.                                       |
| `testar <regra> <versao>`                                                                      | Roda os exemplos. Sai com código 1 se algum falha.               |
| `simular <regra> <versao> <arquivo.txt>...`                                                    | Mostra o que mudaria se a versão entrasse em vigor. Só leitura.  |
| `ativar <regra> <versao>`                                                                      | Ativa. **Só num terminal interativo**, com confirmação digitada. |
| `consolidar <regra> <versao>`                                                                  | Consolida. Idem.                                                 |
| `desativar <regra>`                                                                            | Desativa a versão em vigor. Idem.                                |
| `reverter <regra>`                                                                             | Volta à versão anterior. Idem.                                   |
| `parametro <nome> <AAAA-MM> <arquivo.json> --fonte <url> [--ate AAAA-MM]`                      | Grava um parâmetro versionado. Idem, com confirmação.            |

Os comandos que mudam o que o sbf reconhece (`ativar`, `consolidar`, `desativar`, `reverter`,
`parametro`) não rodam sem terminal interativo. Pedem que você digite exatamente o que o
comando mostra (`regra@versao`, ou só `regra`). Não há opção nem variável de ambiente que
pule essa confirmação.

Sequência típica para um tipo novo:

```text
sbf regras --inventario <cofre> importar comprovante-pix-banco-exemplo.json --motivo "layout do comprovante"
sbf regras --inventario <cofre> exemplo comprovante-pix-banco-exemplo comprovante.txt --esperado esperado.json --nome "comprovante de 10/11"
sbf regras --inventario <cofre> testar comprovante-pix-banco-exemplo 1
sbf regras --inventario <cofre> simular comprovante-pix-banco-exemplo 1 comprovante.txt
sbf regras --inventario <cofre> ativar comprovante-pix-banco-exemplo 1
```

## 7. Ferramentas do MCP

O servidor `sbf mcp` expõe cinco ferramentas para o Claude. Elas aparecem quando o cofre tem
`regras.sqlite`. Só `propor_regra` escreve, e só cria **rascunho** com autor `Ia`.

| Ferramenta       | Parâmetros                                                      | O que faz                                                                                                                   |
| ---------------- | --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `listar_regras`  | nenhum                                                          | Resumo de cada regra (id, nome, tipo, versão em vigor, estado, execuções, taxa de acerto) e as sugestões de consolidação.   |
| `explicar_regra` | `regra`                                                         | Versões (estado, autor, motivo), definição e script da versão em vigor e da última, e exemplos (nome e resultado esperado). |
| `testar_regra`   | `regra`, `versao`                                               | Roda os exemplos da versão; diz para cada um se passou e o que divergiu.                                                    |
| `simular_regra`  | `regra` + `versao`, **ou** `definicao` (+ `script`); `entradas` | Mostra o que mudaria em relação às regras em vigor. Não grava nada.                                                         |
| `propor_regra`   | `regra`, `nome`, `definicao`, `script` (opcional), `motivo`     | **Propõe**: salva um rascunho com autor `Ia`. Propor não ativa. A versão em vigor continua a mesma.                         |

A resposta de `propor_regra` diz: "rascunho criado; ativar é só na janela (área Regras de
reconhecimento)". Se o conteúdo for igual ao da última versão, nada novo é criado e a
resposta diz qual versão já existe.

O servidor não tem ferramenta para ativar, consolidar, desativar ou reverter.

## 8. Onde ativar

A área **Regras de reconhecimento** da janela do sbf é a forma pensada para o dia a dia:
lista das regras, página de cada uma com editor, exemplos, diff da simulação e versões com
rollback. No Reconhecimento, "Testar com este documento" roda a regra no documento aberto, e
"Criar regra a partir daqui" monta um rascunho a partir de uma correção, sem IA. Enquanto essa
área não estiver entregue na sua versão, use o terminal (seção 6). O MCP propõe rascunhos (`propor_regra`) e também ativa, desativa, reverte e consolida (`ativar_regra`, `desativar_regra`, `reverter_regra`, `consolidar_regra`); `reverter_regra` volta atrás.

## 9. Cuidados

- Nenhum documento real, CPF, conta ou número de cartão entra em exemplo, fixture, regra
  publicada no repositório ou log. Use dados fictícios.
- Texto de documento pode conter instruções em linguagem natural (um PDF ou e-mail mal
  intencionado). Nada do que o documento diz vira código ou instrução: só o que a regra
  declara é executado.
- Cálculo tributário depende de tabela válida por competência e de revisão técnica. Um
  alerta de divergência é a saída certa, não uma correção automática da rubrica.

## 10. A biblioteca de fábrica

O sbf traz 16 regras prontas para documentos financeiros brasileiros comuns, todas em
**rascunho** (nenhuma reconhece nada até você ativar). Os arquivos ficam em
`crates/sbf-regras/fabrica/biblioteca/` (`<id>.json`, com a definição e os exemplos
fictícios, e `<id>.luau` quando há script; `_comum.luau` é colado antes de cada script).
`sbf regras ... listar` mostra todas depois da primeira abertura do cofre.

| Regra                                       | Tipo documental                         | Propõe                                              |
| ------------------------------------------- | --------------------------------------- | --------------------------------------------------- |
| `caixa-habitacao-comprovante-prestacao`     | `comprovante-prestacao-caixa-habitacao` | conciliar "Prestação Casa"                          |
| `caixa-habitacao-prestacoes-pagas`          | `prestacoes-pagas-caixa-habitacao`      | nada (resumo por mês)                               |
| `caixa-habitacao-fgts`                      | `uso-do-fgts-caixa-habitacao`           | nada (resumo)                                       |
| `comprovante-pix-caixa` / `-c6` / `-nubank` | `comprovante-pix-<banco>`               | provar o registro                                   |
| `boleto` (geral, prioridade 0)              | `boleto`                                | nada (valida a linha digitável)                     |
| `fatura-cartao-c6` / `-caixa` / `-nubank`   | `fatura-cartao-<banco>`                 | pagar a fatura                                      |
| `conta-neoenergia` / `-vivo` / `-canaa`     | `conta-<fornecedor>`                    | conciliar "Luz" / "Celular Vivo" / "Internet Canaã" |
| `recibo-uber`                               | `recibo-uber`                           | nada (valor em reais)                               |
| `invoice-anthropic` / `invoice-openai`      | `invoice-<empresa>`                     | nada (guarda `moeda` e `valor_usd_centavos`)        |

Regras de banco ou fornecedor têm prioridade 10 e vencem o `boleto` geral (a fatura e a conta
trazem a linha digitável do boleto de pagamento); a de FGTS tem 20. Os Pix de banco só casam
quando a marca do banco aparece nas 3 primeiras linhas do texto (o logotipo do emissor), para
não confundir o banco de destino com o de origem. Dólar nunca vai em `valor_centavos`.
