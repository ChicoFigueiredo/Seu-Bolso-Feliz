# Revisão de segurança da fatia 1 (desktop Rust)

**Data:** 2026-11-13 · **Revisor:** agente revisor de segurança · **Para:** revisão humana
das 06:00 de 2026-11-14

## Escopo e método

Leitura com olho de atacante local (outro usuário na máquina, malware do mesmo usuário,
`cofre.enc` ou PDF malicioso) e de erro de programação, contra a spec 06 (seções
"Chaveiro", "PDF protegido", "Estante e cópia de segurança do cofre") e a ADR-015.

- SBF, branch `feat/desktop-rust`, worktree `seu.bolso.feliz-desktop`, a partir do commit
  `cecd530`:
  - `crates/sbf-chaveiro`;
  - `crates/sbf-pdf`;
  - `crates/sbf-inventario/src/protegidos.rs`;
  - `crates/sbf-estante/src/{copia,rclone,estante,nome}.rs`;
  - `crates/sbf-inventario/src/{banco,decisao,projecao,migracoes}.rs`, só quanto a injeção
    de SQL e ao log só de acréscimo.
- Bancada, commit `a6f5a24`: `crates/bancada-segredos` inteiro, e
  `bancada-armazem/src/somente_acrescimo.rs` e `pragmas.rs`, que o log de decisões usa.

Nenhum arquivo real foi aberto. Todos os testes desta revisão usaram valores fictícios,
gerados na hora: banco, PDF, `cofre.enc`, senhas e CPF de documentação. Os testes ficaram
fora do repositório, numa sonda própria no scratchpad (seção "Testes desta revisão"). O
único resíduo foi um PDF fictício que a sonda S3 gravou fora da pasta de teste, e ele foi
apagado.

## Achados

Nenhum achado Crítico nem Alto. O que a spec exige de mais sensível foi conferido e está
de pé:

- a senha nunca vai no argv;
- nenhuma cópia decifrada vai para o disco;
- senha errada e arquivo adulterado dão o mesmo erro;
- o chaveiro fica fora do cofre;
- o original nunca muda.

Os achados abaixo são de defesa em profundidade, disponibilidade e resíduo em memória.

| # | Gravidade | Onde (arquivo:linha) | Cenário concreto de falha | Confirmado? | Correção sugerida |
| --- | --- | --- | --- | --- | --- |
| A1 | Média | `crates/sbf-estante/src/nome.rs:95`, `:105-107`, `:134-136`; `crates/sbf-estante/src/estante.rs:185-195` | O destino na estante é montado com `data_chave`, `ciclo_id` e `extensao` crus do banco. Só o fornecedor é saneado. Um valor com `..` (bug de um script, decisão gravada por fora, banco restaurado de outra origem) faz o `rclone copyto` gravar **fora** de `SBF/Estante/` e sobrescrever o que estiver lá, porque o `copyto` substitui o destino. O `caminho_seguro` só protege a origem (`caminho_acervo`), nunca o destino. | **Sim** (S3). Com `data_chave='../../FORA'` e `ciclo_id='../../..'`, o destino planejado foi `../../../faturas/../../FORA_banco-alfa_1,00_abcdef01.pdf`. O backend local gravou o arquivo em `/tmp/`, três níveis acima da base da estante. O relatório deu `enviados: 1, falhas: []`, sem nenhum aviso. | Validar na entrada de `documentos_desejados`: `ciclo_id` com `^\d{4}-\d{2}$`, `data_chave` com `^\d{4}-\d{2}-\d{2}$` e `extensao` com `^[a-z0-9]{1,10}$`. Um valor fora do formato vai para `a-classificar/` ou `sem_copia`. Passar também o `destino` final por `caminho_seguro` antes de `planejar`. Teste de regressão: um item hostil não sai da base. |
| A2 | Média | `crates/sbf-estante/src/copia.rs:93-111` | A cópia de segurança, com acervo inteiro e banco com CPF, valores e texto dos documentos, vai para o Drive protegida só por uma senha escolhida pelo usuário. A única regra é "não vazia" (`:94`). Não há `PASSWORD2` (o sal do rclone crypt), então o rclone usa o sal padrão, fixo e igual para todo usuário do rclone. A derivação do rclone crypt é scrypt N=16384, r=8, p=1. Quem obtiver os arquivos do Drive (conta comprometida, o próprio provedor) ataca uma senha fraca offline, com tabela pré-calculável para o sal padrão. | Parcial. O código e `rclone help backend crypt` ("password2 … Optional but recommended") confirmam. O ataque de força bruta não foi executado. | Gerar a senha da cópia aleatória (≥ 128 bits) e guardá-la no chaveiro (`senha:copia-de-seguranca`), mostrando-a uma vez para o usuário guardar fora do computador. Gerar e guardar também um `PASSWORD2` aleatório, passado como `RCLONE_CONFIG_SBFCOPIA_PASSWORD2` pelo mesmo caminho. Se a senha continuar escolhida pelo usuário, exigir um mínimo (por exemplo 16 caracteres). Ver "Decisões humanas", D2. |
| A3 | Baixa | `crates/sbf-chaveiro/src/arquivo_antigo.rs:146-153`, `:205` | O teto de memória do scrypt olha só `128·N·r`. O scrypt aloca também `B = 128·r·p`. Um `cofre.enc` forjado com N=2, r=2²⁰, p=16 passa no teto, aloca 2 GiB além dos 256 MiB e ocupa a CPU por ~20 s antes de dar `SenhaOuArquivoInvalido`. O `std::fs::read` também não tem limite de tamanho: um `cofre.enc` de vários GB é lido inteiro. | **Sim** (S1): VmHWM 2.493.928 KiB e 19,99 s, depois o erro certo. | Limitar também `128·r·p` (e `N·r·p`, o tempo) ao mesmo teto, ou aceitar só os parâmetros que o `segredos.ts` gravava (r=8, p=1, N ≤ 2¹⁷). Ler o arquivo com teto de tamanho (por exemplo 1 MiB: `File::open` + `take`) e tratar o excesso como arquivo inválido. |
| A4 | Baixa | `bancada-armazem/src/somente_acrescimo.rs:36-48` (usado em `crates/sbf-inventario/src/migracoes.rs:93`) | `decisao` recusa UPDATE e DELETE por trigger. Com `recursive_triggers=0`, que é o padrão do SQLite e vale para qualquer conexão de fora (script Bun, `sqlite3`, Datasette), um `INSERT OR REPLACE INTO decisao (id, …)` com um `id` existente **reescreve** a decisão sem disparar nenhum trigger. As conexões do Rust ligam o PRAGMA. A limitação está documentada no módulo, mas a INV-07 promete a garantia sem ressalva. | **Sim** (S5, sobre o SQL que o próprio gerador emite): com `recursive_triggers=0`, o REPLACE é aceito e troca o `depois`. Com um trigger `BEFORE INSERT … WHEN EXISTS (SELECT 1 FROM decisao WHERE id = NEW.id) RAISE(ABORT)`, é recusado nos dois modos, e o INSERT normal continua funcionando. | Acrescentar esse terceiro trigger (`<tabela>_recusa_substituir`) em `gerar_sql_somente_acrescimo`, numa migração nova (a 2 já pode ter sido aplicada), e um teste com `recursive_triggers=0`. |
| A5 | Baixa | `crates/sbf-pdf/src/abertura.rs:258`, `:312`; `crates/sbf-estante/src/rclone.rs:74`, `:97`, `:117` | Há cópias da senha fora do `Zeroizing`, que ficam no heap liberado:<br>• `LoadOptions::with_password` faz `password.to_string()`;<br>• o `lopdf` guarda a senha no `Reader` e a chave do arquivo em `EncryptionState.file_encryption_key: Vec<u8>`;<br>• `sanitize_password` copia a senha;<br>• no rclone, o `String` do stdout do `obscure` cresce por realocação e é copiado por `trim().to_owned()`;<br>• o `Command::env` guarda cópia do valor ofuscado.<br>Um dump de memória, um core file ou o swap sem cifra pode trazer a candidata (CPF ou parte dele) ou a senha da cópia. | Por leitura do código-fonte do `lopdf` 0.47 (`load_options.rs:99-103`, `encryption.rs:208-220`) e do `rclone.rs`. Não foi medido em dump. | No PDF, só o lopdf a montante resolve (`LoadOptions` com `Zeroizing`, `Zeroize` no `EncryptionState`): registrar como limite conhecido e abrir issue no lopdf. No rclone: ler o stdout do `obscure` para um `Zeroizing<Vec<u8>>` com capacidade reservada e evitar o `to_owned`. Desligar core dumps do processo (`prctl(PR_SET_DUMPABLE, 0)`, ou `RLIMIT_CORE=0` no Linux) quando o chaveiro estiver aberto; o lint `unsafe_code = forbid` pede uma crate segura para isso. |
| A6 | Baixa | `crates/sbf-pdf/src/abertura.rs:43-44`; `crates/sbf-estante/src/rclone.rs:29-33` | `pdftotext`, `qpdf` e `rclone` são procurados no `PATH`. Na máquina de teste, o `strace` mostrou a busca passando por `~/.bun/bin`, pelos shims do mise, por `~/.opencode/bin` e por `/usr/local/sbin` antes de `/usr/bin`. Um executável com esse nome plantado numa pasta gravável do `PATH` recebe a senha candidata (`qpdf`, pela entrada padrão), o PDF decifrado (`pdftotext`) ou a senha ofuscada da cópia (`rclone`, pelo ambiente). | **Sim** (S4, `execve` no `strace`): a ordem de busca foi observada. O plantio não foi testado. | Resolver o caminho absoluto uma vez, na abertura, em pastas do sistema (`/usr/bin`, `/usr/local/bin`; no Windows, o caminho instalado). Mostrar esse caminho em Opções e Bastidores, e aceitar outro só por configuração explícita. |
| A7 | Baixa | `crates/sbf-pdf/src/processo.rs:48-55`; `crates/sbf-estante/src/rclone.rs:63-75` | Os filhos herdam o ambiente inteiro do app: 153 variáveis na sessão de teste, o que inclui tokens e chaves de API que o usuário tenha no shell. Variáveis `RCLONE_*` soltas no ambiente também mudam o comportamento do rclone, por exemplo `RCLONE_CONFIG_SBFCOPIA_PASSWORD2` ou `RCLONE_DRY_RUN`, e a cópia pode "dar certo" sem copiar ou com outra chave. | **Sim** (S4): o `strace` mostrou 153 variáveis no `execve`. O efeito de `RCLONE_*` foi conferido só por leitura. | `env_clear()` e repassar só o necessário: `PATH`, `HOME`, `LANG`/`LC_ALL`, `XDG_CONFIG_HOME`, e `RCLONE_CONFIG` se configurado. No rclone, além disso, remover do filho todo `RCLONE_*` que não for do app. |
| A8 | Baixa | `crates/sbf-estante/src/copia.rs:131-137` | O instantâneo do `inventario.sqlite`, em claro, é gravado em `<cofre>/tmp/copia-*/`. Um SIGKILL, um abort ou uma queda de energia deixa a cópia ali, e nada a apaga na abertura. A spec pede essa faxina para a pasta do OCR, mas não para esta. Se o cofre ficar numa pasta sincronizada (OneDrive, Drive para desktop), a sobra sobe em claro. | Por leitura. | Faxina de `<cofre>/tmp/copia-*` na abertura do app. Melhor ainda: gravar o instantâneo numa pasta temporária da pasta de dados do app, fora do cofre. |
| A9 | Baixa | `crates/sbf-estante/src/copia.rs:217-248` | `restaurar` só recusa se `para/inventario.sqlite` existir. O `rclone copy` do acervo para `para/acervo/` sobrescreve arquivos de mesmo nome que já estejam lá. A doc diz "Nunca sobrescreve", mas isso só vale para o banco. Não há limite de tempo em nenhuma chamada do rclone: um remoto travado trava a ação. | Por leitura. | Exigir `para/` vazio ou inexistente, ou usar `--ignore-existing` no acervo. Pôr limite de tempo nas chamadas, como o `sbf-pdf` já faz (`esperar` com prazo). |
| A10 | Baixa | `crates/sbf-pdf/src/processo.rs:78`; `crates/sbf-pdf/src/abertura.rs:197-198`, `:208-210` | Nada limita o PDF lido para a memória nem o stdout do `pdftotext`. O parse e a decifração do `lopdf` rodam no processo, sem prazo. O `lopdf` tem teto de 256 MiB por fluxo descomprimido, mas não teto total. Um anexo hostil (o vetor realista, porque os PDFs chegam por e-mail) pode travar a rodada ou pressionar a memória. Os filhos têm prazo de 30 s. | Não testado (só os limites do `lopdf` foram lidos, em `load_options.rs:15`). | Recusar arquivo acima de um teto (por exemplo 50 MiB), que vira pendência "grande demais". Rodar a abertura numa thread com prazo. Ler o stdout do filho com `take` (por exemplo 8 MiB). |
| A11 | Baixa | `bancada-segredos/src/guarda.rs:335-366`, `:240-260` | Dois processos com o mesmo arquivo de fallback, por exemplo duas janelas ou a janela e um worker, cada um com o mapa em memória: a gravação de um sobrescreve a do outro e um segredo some. `criar_arquivo` tem uma TOCTOU curta entre `existe()` e o `rename`. | Por leitura. | Trava de arquivo (`fs2`/`fd-lock`, ou `rustix::flock`) em torno de ler, mesclar e gravar. Ou reler o arquivo e mesclar dentro da gravação. |
| A12 | Informativa | `crates/sbf-pdf/src/abertura.rs:109-118`, `:289-302` | Um PDF com manipulador de segurança desconhecido (`/Filter /Estranho`) ou revisão inexistente (`/R 9`) vira a pendência **"corrompido"** ("Há outra cópia?"), quando deveria ser **"cifra não suportada"**, porque o qpdf também recusa e o código lê o erro como corrupção. | **Sim** (S4): os dois arquivos deram `Corrompido`. AES-256 R6, AES-128, RC4-128 e RC4-40 abriram pelo `lopdf` com a senha certa e deram `SenhaErrada` com a errada. | Mapear `/Filter ≠ /Standard` e `R` fora de 2 a 6 (lidos do `/Encrypt` sem senha) para `CifraNaoSuportada` antes do fallback. |
| A13 | Informativa | `crates/sbf-pdf/src/abertura.rs:340-363` | O fallback do qpdf depende do AppArmor do Ubuntu: original com nome `*.pdf` e fora de pasta oculta. Cofre ou origem local dentro de pasta oculta, ou nome sem `.pdf`, faz o fallback falhar e o documento ir para "corrompido". | **Sim**: o `qpdf` deu `Permission denied` ao ler um PDF no scratchpad, que fica sob `~/.cache`. | Registrar na tela de pendência o motivo provável. Quando o fallback falhar com `EACCES`, dar mensagem própria. |
| A14 | Informativa | `crates/sbf-pdf/src/rodada.rs:154-166`; `crates/sbf-pdf/src/abertura.rs:361-363` | Em `localizar`, o `caminho_acervo` do banco entra em `raiz.join(...)` sem `caminho_seguro`, ao contrário da estante: um valor absoluto ou com `..` lê qualquer arquivo, e o texto dele vai para `texto`. O caminho vai como argumento posicional do `qpdf`, sem `--`, e um caminho relativo começando com `-` ou `@` seria lido como opção ou arquivo de argumentos. Hoje os caminhos são absolutos. | Por leitura. | Reaproveitar o `caminho_seguro` da estante e exigir caminho absoluto antes do `qpdf`. |
| A15 | Informativa | `crates/sbf-pdf/src/estrategia.rs:163-164` | `Estrategia` é `pub` com campos `pub`. `CpfPrimeiros(0)`/`(12)` ou `CpfUltimos(12)` construídos direto, sem o `FromStr`, entram em pânico no fatiamento. A mensagem não tem segredo, mas derruba a rodada. | Por leitura. | Construtor validado e campo privado, ou `get(..n)` com erro. |
| A16 | Informativa | `crates/sbf-pdf/src/abertura.rs:128-131` | O `catch_unwind` segura o pânico do `lopdf`, mas o hook padrão de pânico escreve a mensagem no stderr. Num app gráfico, esse stderr pode ir parar no journal do usuário. Não há senha nas mensagens do `lopdf` lidas, mas pode haver trecho do PDF. | Por leitura. | Instalar um `panic::set_hook` que escreva só "falha interna ao ler PDF" durante a abertura, ou globalmente. |
| A17 | Informativa | `bancada-segredos/src/arquivo_cifrado.rs:32-34` | Na leitura, o arquivo de fallback aceita Argon2id com até 1 GiB e 64 passadas. Um arquivo forjado faz `destrancar` levar minutos. Limitado e herdado do chess-vault. | Por leitura. | Teto de leitura mais perto do padrão (por exemplo 256 MiB, 10 passadas), ou progresso e cancelamento na tela. |
| A18 | Informativa | dependências | `cargo deny check` na bancada: `advisories FAILED` só por **unmaintained**:<br>• `zvariant` (RUSTSEC-2026-0370);<br>• `zbus_names` (RUSTSEC-2026-0371);<br>• `zvariant_derive` (RUSTSEC-2026-0372).<br>Os três vêm de `keyring 4.2 → zbus-secret-service-keyring-store → zbus 5`. `bans`, `licenses` e `sources` deram ok. No SBF (sem `deny.toml`), `cargo deny check advisories` dá os mesmos três, e nenhuma vulnerabilidade. `cargo audit` não está instalado nesta máquina e não foi rodado. O `cargo deny` usa o mesmo banco RustSec. | **Sim** (execução). | Acompanhar o `keyring` até a migração para o zbus 6. Ignorar os três IDs no `deny.toml`, com comentário e data. Copiar o `deny.toml` da bancada para o SBF, porque sem ele `cargo deny check licenses` sempre falha. |

## O que foi conferido e está correto

### Chaveiro (`sbf-chaveiro`) e importação do `cofre.enc`

- Os casos abaixo dão todos o mesmo `SenhaOuArquivoInvalido`, sem nada lido (S1b e a suíte
  `tests/importacao.rs`):
  - senha errada;
  - etiqueta GCM adulterada;
  - JSON quebrado, base64 inválido ou versão diferente de 1;
  - `alg` diferente de scrypt;
  - N que não é potência de 2, N=2⁴⁰, N=2³⁰ com r=1, p=0.

  Os parâmetros absurdos de N são recusados em ~50 µs, antes de derivar.
- O AAD é `sbf-inventario-segredos-v1`, como no `segredos.ts`. A etiqueta é conferida
  pelo `aes-gcm` antes de qualquer uso do texto claro. A decifração é no próprio buffer
  `Zeroizing`.
- A chave derivada fica em `Zeroizing<[u8; 32]>`. A conferência da importação compara em
  tempo constante (`subtle::ConstantTimeEq`). `cpf_valido` não devolve nem formata o
  número.
- `ErroDoChaveiro` não carrega valor, rótulo nem conteúdo. O `Debug` de `SegredosAntigos`
  e de `Chaveiro` mostra só rótulos ou contagens. O crate não escreve em log.
- A pasta do chaveiro é recusada dentro do cofre, inclusive por `..` e por link simbólico
  (teste `recusa_pasta_dentro_do_cofre_mesmo_por_link_e_ponto_ponto`). Ela vai para
  `XDG_DATA_HOME`/`%APPDATA%`/Application Support, e um `XDG_DATA_HOME` relativo é
  ignorado.
- O `cofre.enc` só é apagado por `OfertaDeApagar::apagar`, que só nasce de uma
  conferência bem-sucedida. O `workers/inventario/src/segredos.ts` saiu do repositório.

### Guarda da bancada (`bancada-segredos`)

- Na cifra:
  - XChaCha20-Poly1305 com nonce aleatório de 24 bytes a cada gravação (teste
    `cada_gravacao_usa_nonce_novo`), sem risco de reuso;
  - o cabeçalho inteiro, com a versão, é o AAD;
  - verificador comparado sem atalho e decifração sempre tentada;
  - senha errada e arquivo adulterado dão o mesmo erro, e só a versão mais nova tem erro
    próprio.
- No formato:
  - o formato 2 é binário de tamanho exato, recusa sobra, rótulo repetido ou inválido e
    quantidade absurda, sem alocar antes;
  - o v1 do chess-vault é lido e regravado como v2, de mão única.
- No disco: diretório criado 0700, arquivos 0600, gravação atômica com `create_new`
  (não segue link plantado), `fsync` do arquivo e do diretório.
- Nos tipos: `Segredo` não tem `Display` nem `PartialEq`, e o `Debug` é `Segredo(***)`.
  `Zeroizing` 1.9 também esconde o valor no `Debug`. `ErroDeSegredo` e `FalhaDoChaveiro`
  têm só mensagens fixas, e os bytes que o keyring devolve num erro são zerados.
- **Keyring ausente:** sem D-Bus (`DBUS_SESSION_BUS_ADDRESS` apontando para lugar nenhum),
  `ChaveiroNativo::esta_disponivel()` dá `false`, a guarda fica `Indisponivel` e
  `gravar` é recusado (S2). Nada cai em texto puro. A hipótese de que a crate `keyring`
  diria "disponível" sem barramento foi **refutada**.

### PDF protegido (`sbf-pdf`, `sbf-inventario/src/protegidos.rs`)

Sob `strace -f` (`execve`, `openat`, `creat`, `rename*`, `unlink*`, `memfd_create`), a
rodada sobre seis PDFs cifrados, com senha certa e errada e com o fallback do qpdf
acionado (S4), mostrou:

- o argv do `pdftotext` é só `-layout -l 3 - -`;
- o do `qpdf` é só `--password-file=- --decrypt <original> -`;
- nenhuma das duas senhas fictícias aparece no log;
- nenhum arquivo é aberto para escrita, criado, renomeado ou apagado, e não há
  `memfd`: nenhuma cópia decifrada vai para o disco e o original só é lido.

Além disso:

- A candidata vive em `Candidata(Zeroizing<String>)`, com `Debug` `Candidata(***)` e sem
  `Display`. É montada com capacidade exata e descartada logo depois da tentativa.
- `tentativa_senha` grava só o nome da estratégia e o resultado. Todo SQL de
  `protegidos.rs` é parametrizado. A transação é IMMEDIATE, curta e sem nada lento
  dentro.
- A lista padrão de estratégias só entra com a opção ligada. A regra aprendida é só uma
  proposta, e vira decisão só com confirmação.
- O limite de falhas vira pendência e as tentativas param.
- `panic = "unwind"` está explícito no perfil de release, então o `catch_unwind` vale.

### Estante e cópia (`sbf-estante`)

- O rclone é chamado sempre por `Command` com lista de argumentos, sem shell. Um destino
  vazio ou começando com `-` é recusado.
- A senha da cópia segue `rclone obscure -`, pela entrada padrão, e depois vai só para o
  ambiente do filho. O teste `a0d75fd` confere que ela fica fora do `cmdline`. O erro do
  rclone leva só o fim do stderr.
- O instantâneo do banco sai da API de backup, sem `-wal`/`-shm`, numa pasta temporária
  0700. É conferido (`integrity_check`), e o hash do nome é conferido na restauração.
- A origem da estante (`caminho_acervo`) passa por `caminho_seguro`, que recusa
  absoluto, `..` e vazio.

### Banco (`banco`, `decisao`, `projecao`, `migracoes`)

- Não há injeção de SQL. Todo valor vai por `params!`. Os únicos identificadores
  interpolados são `tabela` e `chave`, que vêm de `AlvoTipo::tabela()` (estático), e
  `coluna`, que passa por `conferir_coluna`, uma lista fixa por `AlvoTipo`, **antes** de
  entrar no SQL. Isso vale também no `reprojetar`, que lê as chaves do JSON gravado no
  log.
- Os triggers que recusam UPDATE e DELETE em `decisao` são instalados pela migração 2, e
  as conexões do Rust ligam `recursive_triggers` (ressalva em A4).

### Suítes existentes

Rodadas por esta revisão numa pasta de build própria, sem tocar no `target/` do
integrador. Todas passaram:

- `sbf-chaveiro`: unit 6 e `importacao` 13.
- `sbf-estante`: unit 8, `copia` 4, `estante` 5 e `nomes_ts` 3.
- `sbf-pdf`: unit 8, `abertura` 4, `impressao_canonica` 1, `rodada` 7 e `sem_vazamento`
  (com os subprocessos), 1 ignorado.
- `bancada-segredos`: unit 9, `compatibilidade_chess_vault` 5, `estatico` 4,
  `guarda_arquivo` 14, `guarda_chaveiro` 11 e `vazamentos` 7. Os dois de
  `chaveiro_real` ficam ignorados de propósito.

## Decisões que só um humano toma (riscos aceitos ou a aceitar)

- **D1. Malware do mesmo usuário fica fora do modelo.** Qualquer processo do usuário lê
  o Secret Service ou o Keychain destrancado, o `/proc/<pid>/environ` do rclone (com a
  senha ofuscada, reversível por `rclone reveal`), a memória do app e o `PATH`. A ADR-015
  não cobre esse caso. Proponho registrar explicitamente que ele está fora do modelo, e
  tratar A5, A6 e A7 como endurecimento barato, não como garantia.
- **D2. Política da senha da cópia de segurança (A2).** As opções são duas:
  - gerada pelo app, forte, guardada no chaveiro e mostrada uma vez para guardar fora
    (recomendado);
  - escolhida pelo usuário, com mínimo.

  Também é preciso decidir onde fica a cópia fora do computador. Sem o `PASSWORD2`
  aleatório, a cópia no Drive é tão forte quanto a senha escolhida.
- **D3. O cofre está numa pasta sincronizada?** Se estiver, o instantâneo em claro de
  `<cofre>/tmp/` (A8) e qualquer sobra sobem em claro. A ADR-015 já aceita que "um
  arquivo copiado para fora do disco cifrado sai em claro". Confirmar que isso inclui o
  banco.
- **D4. Swap, hibernação e core dumps.** Os resíduos de A5 só não chegam ao disco em
  claro se o swap estiver no volume cifrado. O aviso de disco sem cifra (ADR-015) olha a
  pasta do cofre, não o swap. Decidir se o aviso também deve olhar o swap.
- **D5. Rótulos em claro.** `rotulos.json` e os atributos do Secret Service mostram
  `senha:<rotulo>`, o que revela com quais bancos o usuário tem senha própria. Está
  documentado na bancada. Aceitar, ou usar rótulos opacos (`senha:1`, com o nome dentro
  do valor cifrado).
- **D6. Dependências sem manutenção (A18).** O zbus 5 chega pelo `keyring` 4.2, e não há
  versão segura para subir. Aceitar até a `keyring` migrar.
- **D7. `cargo audit`** não está instalado, e a CI não roda nem ele nem `cargo deny`.
  Decidir se entra na CI da fatia 2.
- **D8. Keyring com D-Bus mas sem provedor de Secret Service** (sessão headless com
  `dbus` e sem `gnome-keyring`). Não foi testado, porque um barramento privado pode
  ativar o `gnome-keyring` do usuário e tocar o keyring real. Fica UNVERIFIED se a guarda
  cai para `Indisponivel` ou para um erro de gravação nesse caso.

## Testes desta revisão (fora do repositório)

Sonda Rust no scratchpad da sessão (`revisao-seguranca/sonda/tests/sondas.rs`), com
dependências por caminho para os crates revisados, mais um script Python
(`revisao-seguranca/scripts/s5_replace.py`) e PDFs gerados na hora. Todos os valores são
fictícios: `UsuarioFicticio1`, `senha-ficticia-1` e o CPF de documentação.

- **S1:** `cofre.enc` forjado com N=2, r=2²⁰, p=16. Resultado: erro certo, 19,99 s, pico
  de 2,4 GiB (A3).
- **S1b:**
  - N=2⁴⁰ e N=2³⁰ com r=1;
  - N=3;
  - p=0;
  - lixo no lugar do JSON.

  Resultado: o mesmo erro em µs.
- **S2:** keyring nativo sem D-Bus. Resultado: `Indisponivel`, gravação recusada.
- **S3:** banco com o esquema real (`0001-base-bun-v2.sql`), um achado `documento` com
  `data_chave` e `ciclo_id` hostis, `Estante::atualizar` com rclone de verdade sobre o
  backend local. Resultado: o arquivo escapou da base (A1). O arquivo fictício gravado
  fora foi apagado.
- **S4:** PDF fictício cifrado pelo `qpdf --encrypt` em seis versões (AES-256 R6,
  AES-128, RC4-128, RC4-40, `/Filter /Estranho`, `/R 9`), aberto por `PdfProtegido` com a
  senha certa e com a errada, sob `strace -f`. Os PDFs ficaram numa pasta não oculta
  dentro de `target/`, por causa do AppArmor, e foram apagados no fim. Resultado:
  - nenhuma senha no argv;
  - nenhum arquivo escrito;
  - classificação descrita em A12;
  - ordem de busca no `PATH` descrita em A6;
  - ambiente herdado descrito em A7.
- **S5:** o SQL de `gerar_sql_somente_acrescimo` num SQLite em memória, com
  `INSERT OR REPLACE`, `recursive_triggers` 0 e 1, com e sem o trigger `BEFORE INSERT`
  proposto (A4).

## Correções de 2026-11-13

Feitas na branch `feat/desktop-correcoes-seguranca` (a partir de `feat/desktop-rust`,
`ec26179`), um commit por achado ou par de achados. Os testes de A1, A3, A4, A6 e A7 foram
vistos falhando antes da correção; os de A2, A8, A9, A10, A14 e A15 usam API nova e não
compilavam antes. Todos os testes usam dados fictícios gerados na hora. As sondas S1, S3 e S5
viraram testes versionados (`seguranca_scrypt.rs`, `seguranca_destino.rs`,
`seguranca_decisao.rs`).

| # | Situação | Commit | Teste de regressão | O que mudou |
| --- | --- | --- | --- | --- |
| A1 | **Corrigido** | `0fb18e1` | `sbf-estante/tests/seguranca_destino.rs`; unit `destinos_que_sairiam_da_base_sao_recusados`, `formatos_de_data_ciclo_e_extensao` | `data_chave` só entra no nome no formato `AAAA-MM-DD`, `ciclo_id` só vira pasta no formato `AAAA-MM` (senão `a-classificar/`) e `extensao` só no formato `[a-z0-9]{1,10}` (senão o nome fica sem extensão). Antes de cada `copyto`, `moveto` e `deletefile`, o destino relativo passa por `destino_seguro` (sem barra inicial, componente vazio, `.`, `..`, `\` nem `:`), e o que falha vai para `falhas` sem chamar o rclone. Para valores válidos o nome é o mesmo do TS (`nomes_ts.rs` passa). |
| A2 | **Corrigido** | `e49a476` | `sbf-estante/tests/seguranca_senha.rs` (e `copia.rs`, o INV-12, continua passando) | `gerar_senha_da_copia() -> Resultado<Segredo>`: 30 símbolos de um alfabeto de 32 (150 bits), em 6 grupos de 5, sem `l`, `o`, `0`, `1`. Senha escolhida abaixo de `MINIMO_DA_SENHA_ESCOLHIDA = 16` caracteres dá `Erro::SenhaCurta`. Cada destino ganha um `PASSWORD2` aleatório (32 bytes em hexadecimal), gravado **em claro** em `sal-da-copia.txt` na raiz do destino (fora da pasta cifrada) na primeira cópia, e passado ofuscado em `RCLONE_CONFIG_SBFCOPIA_PASSWORD2`. Destino com arquivos e sem sal dá `Erro::DestinoSemSal`; sal fora do formato dá `Erro::SalInvalido`. Ver "Decisões das correções", C1. |
| A3 | **Corrigido** | `c1b192e` | `sbf-chaveiro/tests/seguranca_scrypt.rs`; unit `parametros_absurdos_sao_arquivo_invalido` | Aceita só N de 2¹⁰ a 2¹⁸, r de 1 a 16 e p de 1 a 4 (o `segredos.ts` gravava N=2¹⁷, r=8, p=1, e N=2¹⁰ nos testes), com tetos de 256 MiB para `128·N·r` **e** `128·r·p`, e de trabalho `N·r·p ≤ 2·2¹⁷·8`. O `cofre.enc` acima de 1 MiB nem é lido (`take`). Fora disso, o mesmo `SenhaOuArquivoInvalido`, em menos de 200 ms (o caso r=2¹⁸, p=16 levava 5,2 s antes). |
| A4 | **Corrigido** | `70c4df5` | `sbf-inventario/tests/seguranca_decisao.rs` (conexão rusqlite crua, `recursive_triggers=0`) | Migração 2 ganha `GATILHO_QUE_RECUSA_SUBSTITUIR` (`decisao_recusa_substituir`, `BEFORE INSERT … WHEN EXISTS (… id = NEW.id)`). `INSERT OR REPLACE`, `REPLACE` e o upsert com `id` existente são recusados; o `INSERT` normal, com ou sem `id`, continua. A migração 2 foi editada em vez de criar a 3 porque ainda não foi aplicada em nenhum banco real (C4). O trigger ficou no SBF, não em `gerar_sql_somente_acrescimo` da bancada (fora do escopo deste agente; C5). |
| A5 | **Parcial** | `3ea9bc7` | (sem teste automático; é resíduo em memória) | A saída do rclone (a senha ofuscada do `obscure`) agora vai para um `Zeroizing<String>` com 4 KiB reservados, sem realocar, e é zerada ao descartar. Ficam como limite conhecido: a cópia que o `Command::env` guarda, as do `lopdf` (`LoadOptions::with_password`, `EncryptionState`) e os core dumps. |
| A6 | **Corrigido** | `f4c5bd8` (PDF), `ad3e891` (rclone) | `sbf-pdf/tests/seguranca_ferramentas.rs`, `sbf-estante/tests/seguranca_rclone.rs` | `pdftotext`, `qpdf` e `rclone` são resolvidos uma vez por caminho absoluto em `/usr/bin`, `/usr/local/bin`, `/bin` (macOS: mais `/opt/homebrew/bin`, `/opt/local/bin`; Windows: `%ProgramFiles%`), nunca pelo `PATH`. `Ferramentas` e `Rclone::com_binario` continuam como configuração explícita; `Rclone::binario()` e os campos de `Ferramentas` dão o caminho para Opções e Bastidores. Caminho relativo, ausente, ou com o arquivo ou a pasta graváveis por outros (ou pelo grupo, se o grupo não for o do root nem o grupo próprio do dono) não roda: `NaoEncontrada` ou `Erro::RcloneAusente`. Teste com executável falso na frente do `PATH`. |
| A7 | **Corrigido** | `f4c5bd8` (PDF), `ad3e891` (rclone) | os mesmos de A6 | `env_clear()` em todo filho. PDF: só `PATH=/usr/bin:/bin`, `LANG`, `LC_ALL`, `LC_CTYPE` (e no Windows `SystemRoot` e afins). rclone: isso mais `HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `TMPDIR`, `RCLONE_CONFIG`, `RCLONE_CONFIG_PASS`, proxies e `SSL_CERT_*`, e as `RCLONE_CONFIG_SBFCOPIA_*` do app. Nenhum outro `RCLONE_*` do shell passa (testado com `RCLONE_DRY_RUN=true` e um `PASSWORD2` alheio no ambiente do pai). |
| A8 | **Corrigido** | `76f6d74` | `sbf-estante/tests/seguranca_sobras.rs` | `copia::faxinar_sobras(cofre, idade)` apaga `<cofre>/tmp/copia-*` mais velhas que `idade` (não segue link, não mexe em outra coisa). `fazer` chama com `IDADE_DA_SOBRA` (24 h) antes de criar o instantâneo novo. A abertura do app deve chamar também (costura). |
| A9 | **Corrigido** | `76f6d74` | `sbf-estante/tests/seguranca_sobras.rs` | A restauração do acervo usa `--ignore-existing`. As chamadas curtas do rclone (listar, ler o sal, ofuscar, renomear, apagar) têm prazo de 5 min (`PRAZO_PADRAO`, `Rclone::com_prazo`), e passado o prazo o processo é morto e dá `Erro::TempoEsgotado`. `copy` e `copyto` ficam sem prazo de relógio, só com o `--timeout` do próprio rclone. |
| A10 | **Corrigido** (menos o prazo do parse) | `a388359` | `sbf-pdf/tests/seguranca_limites.rs` | `PdfProtegido::ler` recusa acima de `TAMANHO_MAXIMO_DO_PDF` (50 MiB) com `ErrorKind::FileTooLarge`, e a rodada devolve `Situacao::GrandeDemais`. A saída guardada dos filhos para em `SAIDA_MAXIMA_DO_FILHO` (8 MiB), o stderr em 64 KiB, e o resto é drenado. **Falta:** o parse do `lopdf` continua sem prazo, porque uma thread não pode ser cancelada e o parse não tem ponto de parada. |
| A11 | Aberto | — | — | Na bancada (`bancada-segredos`), fora do escopo deste agente. |
| A12 | Aberto | — | — | Informativo; não houve tempo. |
| A13 | Aberto | — | — | Informativo; depende da tela de pendência. |
| A14 | **Corrigido** | `2649809` | `sbf-pdf/tests/seguranca_caminhos.rs` | `rodada::localizar` recusa `caminho_acervo` absoluto ou com `..`, e origem local relativa. O `qpdf` só recebe caminho absoluto. |
| A15 | **Corrigido** | `2649809` | `sbf-pdf/tests/seguranca_caminhos.rs` | `CpfPrimeiros`/`CpfUltimos` fora de 1 a 11 dão `ErroDeFonte::EstrategiaInvalida` (variante nova; o enum já era `non_exhaustive`) em vez de pânico. |
| A16 | Aberto | — | — | Um `panic::set_hook` é global; quem instala é a janela (costura). |
| A17, A18 | Abertos | — | — | Na bancada e na CI, fora do escopo deste agente. |

### Decisões das correções (para conferir)

- **C1. Onde fica o sal da cópia.** A tarefa pedia o `PASSWORD2` "guardado no chaveiro
  junto com a senha ou derivado de forma documentada". Ele ficou **em claro, ao lado da
  cópia** (`sal-da-copia.txt` na raiz do destino). O sal não é segredo: o que ele precisa
  ser é aleatório e único, para que nenhuma tabela pré-calculada sirva. Guardado no
  chaveiro, ele se perderia junto com o computador e a cópia ficaria ilegível mesmo com a
  senha, o que desmentiria o `AVISO_DA_SENHA`. Derivado da senha, ele seria igual para
  todo usuário com a mesma senha, e a pré-computação voltaria. Quem adultera o arquivo no
  Drive só consegue impedir a restauração, o que já poderia fazer apagando a cópia.
- **C2. `gerar_senha_da_copia` devolve `Resultado<Segredo>`**, não `Segredo`: a falha do
  gerador aleatório do sistema vira `Erro::Aleatorio` em vez de pânico.
- **C3. Mínimo de 16 caracteres** para a senha escolhida, contados em caracteres, não em
  bytes. A gerada tem 35.
- **C4. A migração 2 foi editada** em vez de criar uma migração 3, porque o briefing diz
  que ela ainda não foi aplicada em nenhum banco real. Se algum banco já estiver em
  `user_version=2`, ele **não** recebe o trigger novo: aí é preciso uma migração 3 com o
  mesmo `GATILHO_QUE_RECUSA_SUBSTITUIR` (é `CREATE TRIGGER IF NOT EXISTS`, então repetir
  não faz mal).
- **C5. O terceiro trigger ficou no SBF**, em `migracoes.rs`. O lugar natural é
  `gerar_sql_somente_acrescimo` da bancada, que este agente não podia editar. Quando a
  bancada ganhar o trigger com o mesmo nome, o `IF NOT EXISTS` evita a duplicata.
- **C6. Grupo com escrita.** A regra de "pasta gravável por outros" aceita escrita do grupo
  quando o grupo é o do root ou o grupo próprio do dono (gid igual ao uid, o "user private
  group" do Ubuntu e do Fedora). Sem isso, qualquer executável na pasta pessoal seria
  recusado, porque a umask do Ubuntu é 002. No Windows a std não lê ACL: vale só o caminho
  absoluto em `%ProgramFiles%`.
- **C7. Duplicação.** A resolução e o ambiente mínimo estão em `sbf-pdf/src/ferramenta.rs`
  e repetidos em `sbf-estante/src/rclone.rs`, porque os dois crates não dependem um do
  outro e este agente não podia criar crate novo nem mexer no `Cargo.toml` da raiz. Um
  crate `sbf-ferramentas` resolveria.
- **C8. Faxina com idade de 24 h** dentro de `fazer`, para não apagar o instantâneo de uma
  cópia que esteja rodando em outra janela. Uma cópia que leve mais de 24 h ainda corre
  esse risco.
- **C9. `rclone` sem prazo de relógio nas transferências**, porque o acervo pode levar
  horas. Fica valendo o `--timeout` do rclone (5 min sem tráfego).
