# Mensageiros no inventário (WhatsApp e Telegram)

Fontes em massa que entram pelo inventário (ADR-014) e rodam no `sbf worker`. Crate:
`crates/sbf-mensageiros`. Decisões de base: ADR-014 (inventário no desktop), ADR-015
(privacidade) e ADR-017 (WhatsApp vinculado, só leitura, só Linux, risco aceito pelo CEO).

## O que é real e o que é adaptador

| Parte | Estado |
|---|---|
| Trait `FonteDeMensagens`, mapeamento, gravador, checkpoint, ciclo, ligação no worker | **Real**, testado com fonte falsa |
| Importação do export JSON do Telegram Desktop | **Real**, testada com export fictício |
| Sessão do WhatsApp cifrada (AES-256-GCM, chave no chaveiro) | **Real**, testada |
| Protocolo e lado do SBF da ponte do WhatsApp (processo externo, JSON por stdio) | **Real**, testado com canal falso e com um processo `sh` falso |
| Pareamento por QR (fluxo, desenho do QR, Opções › Mensageiros) | **Real** no lado do SBF; o QR vem da ponte |
| A ponte em si (o executável que fala o protocolo do WhatsApp) | **Não existe ainda.** Falta escrevê-la e validá-la com uma conta de teste. |
| Telegram por MTProto (`grammers`) | **Não feito**; o export cobre o caso |

Nada aqui foi conectado ao WhatsApp nem ao Telegram: os testes só usam fontes falsas.

## O que entra no inventário

Só de conversas que o usuário **incluiu** (padrão: nenhuma; uma conversa excluída ganha de
uma incluída).

- **Anexo** (documento, foto): classificado com as mesmas regras de áreas do inventário
  (`sbf_coleta::areas`, como pasta pessoal): documentos e textos copiam, fotos também (a
  conversa foi escolhida a dedo); áudio, vídeo e figurinha ficam de fora.
- **Texto** que parece financeiro: a mesma lista de palavras dos nomes de arquivo
  (comprovante, fatura, boleto, pix, extrato...), mais linha digitável de boleto e Pix copia e
  cola. Vira um `.txt` no acervo.
- Origem: `whatsapp:<conversa>/<mensagem>` ou `telegram:<conversa>/<mensagem>`; o texto de uma
  mensagem que também tem anexo leva `#texto`. `origin_type` é `whatsapp`/`telegram`, a fonte é
  `mensageiro:whatsapp`/`mensageiro:telegram` (tipo `mensageiro`) e `mensagem_ref` é
  `<conversa>/<mensagem>`. O achado traz `data_chave` = dia da mensagem, tipo `mensagem`.
- **Idempotência:** achado por sha256 (o mesmo conteúdo em duas conversas é um achado com
  duas origens) e origem por chave única. Se o checkpoint se perder, relê, mas não duplica e
  nem baixa de novo a mídia de uma origem que já existe.
- **Checkpoint por conversa** em `mensageiros-checkpoints.json` (pasta de dados do app, fora
  do cofre). Uma falha numa mensagem (mídia expirada, disco) para só aquela conversa e deixa o
  checkpoint antes dela.

## Configuração

`mensageiros.json`, na pasta de dados do app (a janela escreve; o worker lê): `ligado`,
`intervalo_minutos` (padrão 30), `whatsapp` e `telegram` (`ligada`, `incluidas`, `excluidas`),
`export_do_telegram` (pasta do `result.json`), `ponte_do_whatsapp` (executável) e `lote`.
O worker só toca nos mensageiros com `ligado: true`; sem conversa incluída não abre sessão
nem lê o export.

## WhatsApp: a ponte

### Por que ponte e não `whatsapp-rust` direto

Conferido em 2026-11-14: `whatsapp-rust` 0.7.0, licença MIT, Rust 1.94+, mantido. Compila,
mas:

- o armazenamento padrão (`sqlite-storage`) usa `libsqlite3-sys` 0.37 e **não convive** com o
  `rusqlite` 0.41 do SBF (dois crates com `links = "sqlite3"`); sem o recurso, é preciso
  implementar o trait `Backend` (sinal, app state, protocolo, segredos, dispositivo: ~90
  métodos) e o `InMemoryBackend` não exporta estado;
- não dá para validar o pareamento nem a leitura de histórico sem conectar uma conta real, o
  que esta rodada proibiu.

Por isso a integração é o **plano B do ADR-017**: um processo à parte fala o protocolo do
WhatsApp, e o SBF só fala JSON com ele. O executável de referência será o `whatsmeow` (Go,
MPL-2.0). Como o contrato é só o protocolo abaixo, a ponte pode ser trocada por uma sobre
`whatsapp-rust` (com `Backend` próprio) sem mexer no SBF.

### Protocolo (uma linha JSON por mensagem, stdin e stdout)

SBF para a ponte (todos com `id`): `iniciar` (com `sessao` em base64, ou sem ela para entrar
em modo de pareamento), `conversas`, `mensagens` (`conversa`, `desde_quando`, `desde_id`,
`limite`), `baixar` (`conversa`, `mensagem`, `destino`), `desparear`, `encerrar`.

Ponte para o SBF: resposta `{"id":N,"ok":true,"dados":...}` ou `{"id":N,"ok":false,"erro":"codigo"}`;
eventos `{"evento":"qr","codigo":"..."}`, `{"evento":"pareado"}`,
`{"evento":"sessao","blob":"<base64>"}` (guardar cifrado, substitui a anterior) e
`{"evento":"desconectado","motivo":"..."}`.

`conversas` devolve `[{"id","nome","grupo"}]`; `mensagens` devolve `[{"id","conversa","quando",
"texto","anexo":{"nome","mime","tamanho"}}]`. O SBF não confia na ordem nem nos filtros da
ponte: reordena e descarta o que não é posterior ao checkpoint.

**Só leitura por construção:** o enum `Comando` não tem nenhum comando que envie, responda,
reaja, marque como lida ou apague. O teste `comandos_sao_so_de_leitura` quebra se alguém
acrescentar um. Os avisos ao usuário saem pelo Telegram (bot), nunca pelo WhatsApp.

### Segredos

- A ponte sobe com **ambiente limpo** (só `PATH` e `HOME`), `stderr` descartado.
- A sessão chega ao SBF como bloco opaco, é cifrada com AES-256-GCM e gravada em
  `whatsapp.sessao` na pasta de dados (nunca no cofre). A chave de 256 bits é sorteada uma
  vez e fica **só no chaveiro** (`mensageiro:whatsapp`, fora da lista de senhas de PDF). Perder
  a chave equivale a desparear.
- Ao subir, a ponte recebe a sessão decifrada **só pelo pipe**; nada em argv, log, SQLite ou
  arquivo temporário. Sem sessão guardada, o worker **não** abre QR: pareamento só pela janela.
- O worker não pede senha: se o chaveiro não abre sozinho (keyring do sistema), o WhatsApp falha
  nessa rodada e o Telegram segue.

### Pareamento e despareamento (Opções › Mensageiros)

Parear exige marcar "Entendo o risco" (o aviso do ADR-017 fica visível) e só funciona em
Linux. O QR é desenhado na janela e renova a cada ~20 s. **Desparear** pede à ponte para
desvincular o aparelho e apaga a sessão e a chave locais; se a ponte falhar, o apagar local
acontece igual (o usuário também pode remover o aparelho no celular).

Limites do vínculo (ADR-017): abrir o WhatsApp no celular a cada 14 dias; estação desligada
por mais de 30 dias perde mensagens.

## Telegram: export do Telegram Desktop

Configurações › Avançado › Exportar dados do Telegram, formato **JSON**, com mídia. Aponte
Opções › Mensageiros para a pasta que tem o `result.json` e escolha as conversas. Aceita o
export completo (`chats.list[]`) e o de uma só conversa. Fica de fora: mensagens de serviço,
figurinhas, GIFs, voz e vídeo, e mídia que o export não baixou. Caminho de mídia com `..`, ou
que sai da pasta por link simbólico, é recusado. Reexportar a mesma pasta, ou uma mais nova,
é seguro (idempotência por origem e sha256); para acompanhar, repita o export e aponte de novo.

### Por que não MTProto agora

`grammers` (MIT OR Apache-2.0) serve, mas exige `api_id` e `api_hash` do usuário, login por
código e senha de duas etapas e uma sessão viva, e nada disso se testa sem conectar na conta
real. TDLib é pesado (C++). O adaptador entra depois pelo mesmo `FonteDeMensagens`; o
checkpoint, o mapeamento e o gravador já servem.

## Ligação no worker

`sbf worker rodar` monta `MensageirosReais` (crate `sbf`), que chama
`sbf_mensageiros::ciclo::rodar` no intervalo de `mensageiros.json`. O estado
(`worker-estado.json`) ganhou `mensageiros_ligados` e a última coleta pode ter origem
`mensageiros`; o log registra só `mensageiros mensagens=N novos=M` e a categoria da falha,
nunca conteúdo.

## Mudanças fora do crate

- `sbf-chaveiro`: `guardar/segredo/tem/apagar_segredo_de_mensageiro` (prefixo `mensageiro:`).
- `sbf-worker`: trait `ColetorDeMensageiros`, `Worker::com_mensageiros`, `Dados::mensageiros()`,
  evento de log e campo de estado.
- `sbf-janela`: módulo `mensageiros` e uma chamada em Opções (`app.rs`, uma linha).
- `sbf`: `MensageirosReais` no `worker.rs` e a dependência.
