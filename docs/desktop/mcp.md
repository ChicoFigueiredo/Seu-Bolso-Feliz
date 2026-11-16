# Modo `sbf mcp`: o Claude consulta o inventário e sugere

O `sbf mcp` é um servidor MCP local (stdio), do binário único (ADR-012, ADR-015 item 4). O Claude
Code ou o Claude Desktop o iniciam como processo filho, pela sua assinatura, e usam as ferramentas
para ler o inventário e o livro-caixa e **propor** classificações. Nada sai da máquina por conta do
servidor, e ele **nunca grava decisão**: a única escrita é `sugerir`, que põe linhas em
`sugestao_ia` (modelo `mcp`) e uma rodada própria em `rodada_ia`. Quem confirma é você, em
Inventário › Triagem por IA.

```text
sbf mcp --inventario <pasta-do-cofre> [--livro <livro-caixa.sqlite>] [--planejamento <arquivo>]
```

- Abre só `<pasta-do-cofre>/inventario.sqlite`, que tem de existir e estar migrado pelo app
  (versão 3 em diante). Nada é criado.
- O livro-caixa vem do cache local da estação (o mais recente em
  `<pasta de dados>/livro-caixa/*/livro-caixa.sqlite`), em leitura. `--livro` indica outro arquivo.
  Sem cache, `livro_movimentacoes` responde que não há.
- O stdout é só do protocolo. O stderr recebe uma linha por chamada com o nome da ferramenta e o
  tamanho da resposta, nunca o conteúdo.

## Registrar no Claude Code

```bash
claude mcp add sbf -- sbf mcp --inventario <cofre>
```

(Se o `sbf` não está no PATH, use o caminho completo do binário.)

## Registrar no Claude Desktop

Em `claude_desktop_config.json` (Configurações › Desenvolvedor › Editar configuração):

```json
{
  "mcpServers": {
    "sbf": {
      "command": "/caminho/para/sbf",
      "args": ["mcp", "--inventario", "/caminho/do/cofre"]
    }
  }
}
```

Reinicie o Claude Desktop depois de salvar.

## Ferramentas

Todas devolvem JSON enxuto. As listas são paginadas (`limite` de 1 a 100, padrão 25; `deslocamento`;
a resposta traz `total` e `proximo_deslocamento` enquanto houver mais). Valores em centavos.

| Ferramenta            | Para quê                                                                            |
| --------------------- | ----------------------------------------------------------------------------------- |
| `inventario_resumo`   | Totais por estado, registros sem categoria, decididos por você, sugestões abertas.  |
| `buscar_registros`    | Filtros: `texto`, `fornecedor`, `categoria`, `de`/`ate`, `estado`, `sem_categoria`. |
| `ver_registro`        | Um registro com sub-registros, sugestões abertas e decisões.                        |
| `buscar_achados`      | Filtros: `estado`, `tipo`, `texto` (arquivo, assunto, fornecedor).                  |
| `ver_texto_do_achado` | Texto extraído, truncado como na triagem (3000 do começo, 1000 do fim).             |
| `fornecedores`        | Contagem, quantos sem categoria e a categoria que você já fixou.                    |
| `decisoes_recentes`   | O log das suas decisões, para o Claude imitar o seu critério.                       |
| `livro_movimentacoes` | Movimentações do cache do livro-caixa, com categoria e conta por nome.              |
| `sugerir`             | Grava sugestões `{alvo_tipo, alvo_id, campo, valor, confianca, justificativa}`.     |
| `sugestoes_abertas`   | O que está esperando a sua confirmação.                                             |

`sugerir` valida item a item e devolve o motivo de cada recusa. Recusa alvo inexistente, item que
você já decidiu, registro promovido, campo fora da lista da triagem (achado: `e_financeiro`,
`tipo_documento`, `fornecedor`, `data_chave`, `valor`; registro: `categoria`, `fornecedor`, `conta`,
`tipo`, `duplicata_de`), valor inválido, confiança fora de 0 a 1 e justificativa com mais de 140
caracteres. Uma sugestão nova do MCP para o mesmo alvo e campo torna a anterior obsoleta.

## Controle mensal (spec 11)

Quando existe planejamento (`<cofre>/planejamento.sqlite`, ou o arquivo dado em
`--planejamento`), o servidor liga mais 14 ferramentas. Sem planejamento elas nem aparecem na
lista. Todas usam só o serviço `Planejamento`: leem, simulam e **propõem**. Valores em centavos;
datas em AAAA-MM-DD; competências em AAAA-MM.

| Ferramenta                         | Para quê                                                                                                                |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `listar_compromissos`              | Ocorrências do ciclo ou mês: `periodo` (ciclo, mes), `referencia`, `filtro`, `busca`, `pagina`, `por_pagina` (até 100). |
| `resumo_de_caixa`                  | Números do topo: entradas, saídas de caixa, em atraso, livre após compromissos, lacunas.                                |
| `projetar_compromissos`            | Totais mês a mês: `inicio`, `meses` (até 360).                                                                          |
| `obter_compromisso`                | Regra, versões, pausas, próximas ocorrências e histórico curto. Aceita `rec:AAAA-MM[:pN]` ou o id do recorrente.        |
| `simular_mudanca`                  | Meses afetados, antes e depois de uma mudança. Não grava nem propõe.                                                    |
| `listar_propostas`                 | As propostas do MCP e o estado de cada uma.                                                                             |
| `propor_compromisso`               | Compromisso novo.                                                                                                       |
| `propor_alteracao`                 | Muda valor ou dados: `so_este_mes` (só o valor) ou `daqui_em_diante`.                                                   |
| `propor_pausa` / `propor_retomada` | Pausa por período (`ate` exclusivo) e retomada.                                                                         |
| `propor_cancelamento`              | Marca como cancelado a partir de um mês (só no app, nada no fornecedor).                                                |
| `propor_exclusao_logica`           | Exclui com motivo; o histórico fica.                                                                                    |
| `propor_conciliacao`               | Sugere ligar uma ocorrência a um movimento real (nunca confirmada pelo MCP).                                            |
| `propor_excecao`                   | Adiar, dispensar, pago informado ou valor só deste mês.                                                                 |

**Propostas.** Toda `propor_*` exige `chave` (idempotência, até 120 caracteres): a mesma chave
devolve a mesma proposta e, se o conteúdo for outro, avisa em vez de criar. A resposta diz
"proposta pendente: confirme na janela do SBF" e traz a simulação. A proposta leva a versão
atual do compromisso; se ele mudar até a confirmação, a janela recusa (AC-31).

**Nunca no MCP:** aplicar, confirmar ou recusar proposta, SQL, pagamento ou cancelamento no
fornecedor. Nenhuma chamada muda o estado além de criar uma proposta pendente (AC-30).

**Limites.** `meses` até 360; `por_pagina` de 1 a 100; nomes até 120 caracteres, motivos até
200, notas até 500, busca até 100; valores de 0 a 100 bilhões de centavos. Os erros vêm em
português e a ferramenta recusa o argumento em vez de ajustá-lo em silêncio.

**Privacidade (AC-36).** Nomes, contas, fornecedores, notas e motivos passam pelo nível de
confiança do cliente (mascarado por padrão: CPF, conta, PIX e outros números longos viram
`•••`). Cada chamada grava uma linha em `saida_ia` com destinatário `mcp:<ferramenta>`, o nível
e o tamanho, nunca o conteúdo. O stderr recebe só o nome e o tamanho.

## Fluxo

1. No Claude, peça: "Olhe os registros sem categoria do Mercado Delta e sugira a categoria, seguindo
   o que eu já decidi."
2. O Claude lê `fornecedores`, `decisoes_recentes` e `buscar_registros`, e chama `sugerir`.
3. No SBF, abra Inventário › Triagem por IA. As sugestões do MCP aparecem na mesma lista das da
   rodada de IA, agrupadas por campo e valor. Confirme em lote, ou recuse.
4. Só a confirmação vira decisão no log, com `decidido_por = usuario`.

## Privacidade

O Claude lê, pela sua conta, o que as ferramentas devolvem (descrições, fornecedores, trechos de
texto de documentos). Por isso o servidor só abre a pasta que você deu e só roda enquanto o Claude
o mantém aberto. Não registre o servidor em um Claude que você não queira que veja esses dados.
