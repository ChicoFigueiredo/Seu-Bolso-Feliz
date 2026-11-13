# Spike: abrir PDF protegido sem expor a senha

Data: 2026-11-12. Decide o item 3 da seção "PDF protegido (Apêndice V6)" da
[spec da fatia 1](../06-desktop-fatia-1-inventario.md): abrir dentro do processo
(`lopdf`) ou via `qpdf --password-file=-`. Alimenta o ticket T8 (`sbf-pdf`).

## Recomendação

**Opção C:** o `lopdf` decifra o original em memória, o PDF decifrado vai por stdin para
`pdftotext -layout -l 3 - -`, e nenhuma senha nem cópia decifrada sai do processo para o
disco ou para um argumento. O texto é idêntico byte a byte ao `pdftotext` que o
inventário usa hoje, e não existe cópia temporária para a guarda RAII apagar.

## Requisitos

- A senha candidata nunca vai em argv (visível em `/proc/<pid>/cmdline` e no `ps`), nem
  em log, nem em arquivo.
- O original nunca muda.
- Se houver cópia decifrada, ela fica numa pasta 0700 e some mesmo com falha no meio.
- O texto das 3 primeiras páginas sai com qualidade comparável a `pdftotext -layout -l 3`.
- Dá para distinguir "senha errada" de "PDF corrompido".
- Funciona em Linux, Windows e macOS.

## Como foi medido

O código fica fora do repositório, num projeto cargo próprio (Rust 1.98, edição 2024,
`unsafe_code = "forbid"`): `src/main.rs`, os scripts `gerar_base.py` e
`gerar_fixtures.sh` e os fixtures gerados. Ambiente: Ubuntu, qpdf 12.3.2, poppler
26.01.0, lopdf 0.47.0, pdf-extract 0.12.1.

**Fixtures, todos fictícios.** São PDFs de 4 páginas gerados com reportlab: título,
"Vencimento 10/12/2026 — Valor R$ 1.234,56", nome com acentos, linha digitável e uma
tabela de 3 colunas (data, descrição e valor alinhado à direita). A marca
`MARCA-PAGINA-4` só pode aparecer se a extração passar da página 3.

- Há dois sabores de fonte: Helvetica base14 e DejaVuSans embutida em subconjunto
  (Identity-H com ToUnicode, como nos PDFs de banco).
- Cada sabor foi cifrado com `qpdf --encrypt` em RC4-40 (R2), RC4-128 (R3),
  AES-128 (R4), AES-256 (R6) e AES-256 com object streams.
- Senhas: `52998224725` (o CPF de exemplo que aparece em documentação) e `29822`.
- Corrompidos: lixo com cabeçalho `%PDF`, um AES-256 truncado em 3000 bytes e um AES-256
  com o `/U` (verificador da senha) danificado.

**Qualidade do texto.** A referência é `pdftotext -layout -l 3` sobre o PDF não cifrado.
Para cada opção, a medição registra:

- se o texto é igual byte a byte à referência;
- a porcentagem de linhas iguais, exata e com espaços normalizados;
- os 7 campos-chave;
- se data, descrição e valor de uma compra caem na mesma linha;
- se a página 4 ficou de fora.

**Vigia de senha.** Cada processo filho é inspecionado enquanto está vivo e bloqueado à
espera do stdin, antes de a senha ser escrita:

- `/proc/<pid>/cmdline` e `/proc/<pid>/environ` do filho;
- uma varredura de todos os `/proc/*/cmdline` do sistema;
- a espera vai até o exec terminar: logo depois do `spawn` o cmdline às vezes vem vazio,
  e na primeira rodada isso gerou leituras vazias.

O controle positivo foi um processo com `--password=29822` no argv: apareceu no
`/proc/*/cmdline` e no `ps`, o que valida o método.

**Disco.** Um `strace` (`openat`, `creat`, `memfd_create`, `unlink`, `rename`) rodou
sobre `qpdf --password-file=- --decrypt <orig> - | pdftotext -layout -l 3 - -`. O
original é relido e comparado depois de cada rodada.

## Opções

- **A1 (`pdf-extract` nativo):** `pdf_extract::extract_text_from_mem_by_pages_encrypted`,
  que decifra com o lopdf 0.42 embutido no pdf-extract 0.12.1.
- **A1b (`lopdf` 0.47 + `pdf-extract`):** o lopdf 0.47 decifra
  (`load_mem_with_options(LoadOptions::with_password)`), grava num `Vec` e o pdf-extract
  extrai desses bytes.
- **A2 (`lopdf` sozinho):** decifra com o lopdf 0.47 e extrai com
  `Document::extract_text`.
- **B1 (`qpdf` com cópia em disco):** `qpdf --password-file=- --decrypt` grava a cópia
  numa pasta 0700 de `tempfile::Builder` (guarda RAII); depois
  `pdftotext -layout -l 3 <cópia> -`.
- **B2 (`qpdf` em pipe):** `qpdf --password-file=- --decrypt <orig> -` ligado direto ao
  `pdftotext -layout -l 3 - -`. A cópia decifrada só existe no pipe.
- **C (`lopdf` + `pdftotext` por stdin):** o lopdf 0.47 decifra em memória, `save_to` grava
  num `Zeroizing<Vec<u8>>` e os bytes vão pelo stdin do `pdftotext -layout -l 3 - -`. O
  `pdftotext` nunca recebe senha.

`pdftotext` sozinho não serve. Só aceita senha por `-upw`/`-opw`, que vão no argv, e não
tem opção para ler a senha de arquivo ou do stdin. Mas ele lê o **PDF** do stdin (`-`), e
é isso que C e B2 usam.

O pdfium (`pdfium-render`, MIT OR Apache-2.0) não foi medido. Ele exige distribuir a
`libpdfium` por plataforma (cerca de 5 a 8 MB), e a extração de texto dele não tem modo
layout. Fica como alternativa só se o poppler virar problema no Windows.

## Resultados

13 fixtures × 6 opções × (senha certa e senha errada). Os tempos são a mediana de 15
rodadas por documento, em ms.

| Opção                        | Decifra RC4-40/128, AES-128/256, objstm | Senha no argv/environ   | Cópia decifrada em disco | Texto contra `pdftotext -layout`                              | Senha errada × corrompido | Tempo (ms)     | Multiplataforma                                               | Licença                                        | Dependência externa |
| ---------------------------- | --------------------------------------- | ----------------------- | ------------------------ | ------------------------------------------------------------- | ------------------------- | -------------- | ------------------------------------------------------------- | ---------------------------------------------- | ------------------- |
| A1 pdf-extract nativo        | "abre", mas **texto vazio, sem erro**   | não (em processo)       | não                      | 0/7 campos, 0% das linhas                                     | sim (via lopdf)           | 0,6–14         | sim                                                           | MIT                                            | nenhuma             |
| A1b lopdf 0.47 + pdf-extract | sim, 10/10                              | não (em processo)       | não                      | 7/7 campos, 100% das linhas normalizadas, colunas perdidas    | sim                       | 8–15           | sim                                                           | MIT; closure MIT/Apache/BSD-3/Unicode-3.0/Zlib | nenhuma             |
| A2 lopdf `extract_text`      | sim, 10/10                              | não (em processo)       | não                      | 7/7 campos, mas cada coluna numa linha (55%), linha desfeita  | sim                       | 1–8            | sim                                                           | MIT                                            | nenhuma             |
| B1 qpdf → tmp 0700           | sim, 10/10                              | não (stdin)             | **sim**                  | idêntico byte a byte                                          | sim (stderr)              | 62–76          | binários de qpdf e poppler; **AppArmor no Ubuntu** (abaixo)   | qpdf Apache-2.0; poppler GPL (processo à parte) | qpdf + pdftotext    |
| B2 qpdf → pipe → pdftotext   | sim, 10/10                              | não (stdin)             | não (só pipe)            | idêntico byte a byte                                          | sim (stderr)              | 61–66          | binários de qpdf e poppler; AppArmor restringe a leitura      | idem                                           | qpdf + pdftotext    |
| **C lopdf → stdin pdftotext** | **sim, 10/10**                         | **não (nem chega ao filho)** | **não**             | **idêntico byte a byte**                                      | **sim (erro tipado)**     | **35–46**      | lopdf em todo lugar; pdftotext já é dependência do inventário | MIT (+ poppler em processo à parte)            | pdftotext           |

O vigia inspecionou 329 processos filhos, todos lidos vivos, e fez 329 varreduras
completas de `/proc`. Achou **zero** ocorrências da senha em cmdline ou environ. Com
`--password-file=-` o `ps` mostra só `qpdf --password-file=- --decrypt <arquivo> -`.

O `strace` de B2 não mostrou nenhum `open` para escrita, `creat`, `memfd_create`, `unlink`
nem `rename`, nem no `qpdf` nem no `pdftotext`. O mesmo vale para o `pdftotext` de C. O
original ficou byte a byte igual em todas as rodadas.

### Senha errada × corrompido

| Caso                                     | lopdf (A1b, A2, C)                                                | qpdf (B1, B2)                                                                          |
| ---------------------------------------- | ----------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Senha errada                             | `Error::InvalidPassword` (tipado)                                 | rc=2 e `invalid password` no stderr (texto em inglês, não traduzido)                   |
| Lixo com cabeçalho `%PDF`                | `failed parsing cross reference table`                            | rc=2 e outra mensagem                                                                  |
| Truncado (o `/Encrypt` some com o trailer) | abre reparado (`xref_recovery = Some(Reconstructed)`) e sem texto | rc=3 (avisos: "file is damaged") e sem texto                                          |
| `/U` danificado                          | `InvalidPassword` **mesmo com a senha certa**                     | `invalid password` **mesmo com a senha certa**                                         |

Duas regras saem disso.

- **Reparado e sem nenhuma letra conta como corrompido.** Sem essa regra, todas as opções
  "abrem" o truncado e devolvem texto vazio. Com ela, as seis acertam.
- **`/U` ou `/O` danificado é indistinguível de senha errada em qualquer ferramenta.** O
  `/U` é o próprio verificador da senha. Quem segura esse caso é o limite de N falhas, que
  vira a pendência "PDF protegido sem regra".

Também não dá para usar `qpdf --requires-password`: ele devolve 2 ("não cifrado") também
para lixo, então só serve junto com o `--decrypt`.

### `qpdf --password-file=-`

- A ajuda instalada (`qpdf --help=--password-file`, `man qpdf` 12.3.2) diz só "the first
  line of the specified file is used as the password" e não menciona `-`.
- O manual oficial (qpdf.readthedocs.io, CLI) documenta: "_filename_ may be `-` to read
  the password from standard input, but if you do that the password is echoed and there is
  no prompt". O eco só existe em terminal interativo; num pipe não há eco.
- Medido: senha com `\n`, com `\r\n` ou sem terminador é aceita do mesmo jeito. Espaços nas
  pontas não são removidos.
- O qpdf lê a senha antes de abrir o PDF: fica bloqueado no stdin até receber a linha. Foi
  essa janela que o vigia usou.

### AppArmor do qpdf no Ubuntu: problema concreto para B1

O Ubuntu traz o perfil `/etc/apparmor.d/qpdf`, que confina `/usr/bin/qpdf`. Ele só deixa
ler e gravar `*.pdf`, `*.json` e `*.qdf` (sem diferenciar maiúsculas) nestes lugares:

- pastas **não ocultas** da home (`@{HOME}/[^.]**`);
- `/tmp`, `/var/tmp`, `/mnt` e `/media`.

Medido, com `apparmor="DENIED" profile="qpdf"` no log do kernel:

- original ou cópia em `~/.cache/...`: `Permission denied`;
- cópia em `/tmp/.../copia`, sem `.pdf`: `Permission denied`;
- nem `qpdf /dev/stdin < arquivo` contorna, porque o kernel revalida o caminho real.

Consequências:

- **B1 não consegue gravar na pasta de dados do app** (`~/.local/share/...`, oculta), que
  é onde a spec manda pôr a cópia. Só funciona em `/tmp`, o que contraria a spec.
- B1 e B2 só leem originais com extensão `.pdf` em lugar permitido. O acervo grava
  `acervo/ab/<hash>` **sem extensão** quando ela não é conhecida, e esses arquivos o qpdf
  não abre.
- C não roda o qpdf, então não é afetado. O `pdftotext` não tem perfil.

### Guarda RAII da pasta temporária (só vale para B1 ou para um OCR futuro)

`tempfile::Builder::new().prefix("sbf-pdf-").permissions(0o700).tempdir_in(base)`, com um
filho que cria a pasta, grava `decifrado.pdf` e então falha:

| Como o processo morreu                                       | Pasta 0700 | Sobrou depois                       |
| ------------------------------------------------------------ | ---------- | ----------------------------------- |
| retorno normal ou `?` no meio (5 opções × 26 rodadas)        | sim        | não (0 sobras)                      |
| `panic!` com `panic = "unwind"` (padrão)                     | sim        | **não**: o Drop roda no unwind      |
| `std::process::abort()`, equivalente a `panic = "abort"`     | sim        | **sim**                             |
| SIGKILL                                                      | sim        | **sim**                             |

Também deixam sobra, pelo mesmo motivo: `std::process::exit`, queda de energia e OOM
killer. Isso não é testado; é a semântica conhecida de destrutores que não rodam.

A garantia possível é a **faxina na abertura**: apagar todo `sbf-pdf-*` da base privada
antes de começar. No teste, a faxina removeu as 2 sobras. Para pegar o caso do pânico, o
perfil release do `sbf` não pode usar `panic = "abort"`. C e B2 não deixam nada para
faxinar.

## Por que C

- **A senha nunca sai do processo do `sbf`.** Em C, o `pdftotext` recebe só o PDF
  decifrado pelo stdin. Em B, a senha atravessa um pipe e fica na memória do `qpdf` até ele
  terminar. Seguro, mas é mais uma cópia.
- **Não há cópia decifrada em disco.** O SIGKILL deixa de ser problema e a guarda RAII nem
  é necessária.
- **O texto é o do inventário de hoje.** É o mesmo `pdftotext -layout -l 3`, então os
  parsers por fornecedor e o `canonical_fingerprint` veem o mesmo texto do PDF sem senha.
- **O erro é tipado** (`lopdf::Error::InvalidPassword`), sem casar texto de stderr.
- **Não acrescenta dependência externa.** O `pdftotext` já é exigido pelo inventário, e o
  `qpdf` sai da lista.
- **Não esbarra no AppArmor do qpdf.**
- É a mais rápida das que preservam o layout: 35 a 46 ms contra 61 a 76 ms.

A1b fica como plano B se o `pdftotext` faltar numa plataforma. Acha todos os campos, mas
perde o alinhamento de colunas e é mais lenta. A1 (`pdf-extract` nativo) fica **rejeitada**,
porque devolve texto vazio sem erro. A2 fica rejeitada porque separa data, descrição e valor
em linhas diferentes. B1 também fica rejeitada: deixa cópia em disco e esbarra no AppArmor.

## Riscos

1. **O lopdf não zera a senha.** `LoadOptions::with_password(&str)` faz `to_string()`, o
   `Reader` clona, e `sanitize_password` devolve um `Vec` comum; nada disso é zerado (o
   lopdf não depende de `zeroize`). O `Document` decifrado também fica na heap. O `sbf`
   zera a candidata e o `Vec` decifrado, mas as cópias internas ficam até a memória ser
   reutilizada. É memória do próprio processo, do mesmo usuário: aceitável para a fatia 1,
   registrado como limite. A saída é um PR no lopdf que aceite `&[u8]` e zere a memória.
2. **Reserialização do lopdf.** C entrega ao `pdftotext` um PDF regravado pelo lopdf, não
   o original. Nos 10 fixtures o texto saiu idêntico, mas PDFs reais tortos podem divergir.
3. **Cifra que o lopdf não implementa** (handler de chave pública `/Adobe.PubSec`, revisões
   exóticas): devolve `UnsupportedEncryption`. Raro em fatura e boleto.
4. **Windows e macOS não foram testados aqui.** O ponto em aberto é se o `pdftotext` lê
   stdin em modo binário no Windows. No Windows e no macOS o argv também é visível a outros
   processos do mesmo usuário (PEB/WMI, `ps`), então a regra "nada no argv" vale igual.
5. **PDF protegido escaneado** (só imagem) decifra bem e dá texto vazio sem reparo: é caso
   de OCR, não de corrupção. O `ocrmypdf` cria arquivos temporários próprios com as
   páginas decifradas. Se o T8 ou uma fatia seguinte ligar OCR em protegido, isso volta a
   exigir pasta 0700, `TMPDIR` apontado para ela e faxina.
6. **Licenças.** O `lopdf` é MIT. A árvore com `default-features = false` (64 crates) puxa
   BSD-3-Clause (`alloc-no-stdlib`, `alloc-stdlib`, via `brotli-decompressor`) e
   Unicode-3.0 (`unicode-ident`, `unicode-normalization` e `unicode-properties`), além de
   MIT/Apache. Conferir contra a allowlist do `cargo deny` do Chess Studio. O poppler é
   GPL, mas roda como processo à parte, como hoje.

## Para o ticket T8

- Implementar a abertura em `sbf-pdf` pela opção C:
  - `lopdf = { version = "=0.47.0", default-features = false }`;
  - `load_mem_with_options(bytes, LoadOptions::with_password(&candidata))`;
  - `save_to` num `Zeroizing<Vec<u8>>`;
  - `pdftotext -layout -l 3 - -` com stdin em pipe, timeout e kill, como o
    `TIMEOUT_PDF_MS` de `e3-extrair.ts`.
- Desfechos da tentativa:
  - **Aberto** com texto;
  - **Aberto com aviso** (`xref_recovery` preenchido);
  - **SenhaErrada** (`Error::InvalidPassword` e `Decryption(IncorrectPassword)`);
  - **Corrompido** (erro de parse, ou reparado e sem nenhuma letra);
  - **SemTexto** (abriu sem reparo e veio vazio, candidato a OCR);
  - **CifraNaoSuportada**.
- Gravar o desfecho em `tentativa_senha.resultado`. Só SenhaErrada conta para o limite de
  N falhas.
- Rodar o `lopdf` dentro de `catch_unwind`, porque um PDF hostil não pode derrubar a
  janela. Manter `panic = "unwind"` no perfil do `sbf`.
- Testes:
  - os fixtures fictícios deste spike (gerados por script, com o CPF de exemplo
    529.982.247-25) nas mesmas 5 cifras e nos 3 corrompidos;
  - texto igual ao `pdftotext` do PDF sem senha;
  - no Linux, o teste do vigia de `/proc` com o filho bloqueado no stdin;
  - o original igual antes e depois.
- CI no Windows e no macOS. Confirmar `pdftotext - -` com stdin binário no Windows e
  decidir como distribuir o poppler lá. Se falhar, cair para A1b nessa plataforma.
- Não usar `qpdf` no caminho principal. Se alguma cifra não suportada pelo lopdf aparecer
  no acervo real, o fallback é B2, com a senha por stdin e só para originais `.pdf` em
  lugar que o AppArmor permita. B1 nunca.
- A pasta 0700 com faxina na abertura só nasce junto com o OCR de protegido. Não é
  necessária para C.
- Conferir a allowlist do `cargo deny` contra BSD-3-Clause e Unicode-3.0.
