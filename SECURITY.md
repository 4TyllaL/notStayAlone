# Segurança do !StayAlone

O !StayAlone fica o dia todo rodando no PC de quem usa. Este documento descreve o que
ele protege, de quem, e como — organizado no estilo de um *Security Target* da
**Common Criteria (ISO/IEC 15408)**: ambiente, ameaças, objetivos e requisitos
funcionais (SFRs). É uma análise inspirada no método, **não uma certificação**.

## Alvo da avaliação (TOE)

| Parte | O que faz | Toca em rede? |
|---|---|---|
| `dontStayAlone.exe` | mascote, lembretes, configurações, criador de mascotes | não: o processo do mascote nunca usa a rede |
| `dontStayAlone.exe --ia` | conversa com IA (API padrão OpenAI), num processo separado e de vida curta | sim, só quando você conversa |
| `--procurar-versao` / `--baixar-versao` | procura e baixa a versão nova (releases do GitHub) | sim, uma vez por dia (se ligado) |
| `--galeria` / `--instalar-galeria` | lista e instala mascotes da galeria da comunidade | sim, só quando você abre a Galeria |
| Plugins de terceiros | programas que você liga na aba Plugins | o que o plugin fizer (fora do TOE) |

Dados guardados: `%APPDATA%\StayAlone\` (`config.ini`, `state.ini`, `chat.ini`,
`memoria.txt`, `mascots\`, `plugins\`, `update\`) e a chave da API no Gerenciador de
Credenciais do Windows.

## Hipóteses sobre o ambiente

- **A.USUARIO** — roda como usuário comum (`asInvoker`); nunca pede administrador.
- **A.SO** — Windows 10/11 atualizado; o que o próprio Windows protege (DPAPI, TLS do
  WinHTTP, isolamento entre usuários) é confiável.
- **A.CONTA** — quem já executa código com a conta do usuário está fora do escopo: esse
  atacante poderia, de qualquer forma, trocar o próprio `.exe`.

## Ameaças consideradas

| Id | Ameaça |
|---|---|
| T.MENSAGEM | Outro programa da sessão posta mensagens forjadas para as janelas do app. |
| T.MOD | Um mascote baixado da internet (`mascot.txt`/`phrases.txt`) vem malformado ou gigante. |
| T.CHAVE | A chave da API vaza (arquivo, variável de ambiente herdada, memória, rede). |
| T.REDE | Alguém na rede lê ou altera a conversa (Wi-Fi público, proxy). |
| T.SERVIDOR | Um servidor (ou IA) malicioso manda resposta gigante, aninhada ou com caracteres de controle. |
| T.DLL | Uma DLL falsa deixada na pasta do `.exe` (ex.: Downloads) é carregada no lugar da do Windows. |
| T.RECURSOS | Algo faz o app consumir CPU/memória/objetos GDI sem limite. |
| T.PRIVACIDADE | O app registra o que você digita ou envia dados sem você pedir. |
| T.PLUGIN | Um plugin é ligado sem você saber, ou o arquivo dele é trocado depois de aprovado. |
| T.ATUALIZACAO | Um `.exe` falso ou antigo chega pela atualização automática (servidor ou rede comprometidos). |
| T.GALERIA | Um item da galeria vem adulterado, tenta gravar fora da pasta dele ou traz um programa. |
| T.MEMORIA | A memória da conversa guarda dados sensíveis. |

## Requisitos funcionais e como são atendidos

| SFR (família CC) | Ameaça | Como |
|---|---|---|
| **FPT_ITT / FDP_IFF** — separação de dados entre processos | T.MENSAGEM | Nenhuma mensagem do Windows carrega ponteiro. Os dados ficam no `mailbox` dentro do processo; mensagem forjada acha a caixa vazia e é ignorada (`src/mailbox.rs`). Comandos remotos (`--bolinha` etc.) só aceitam uma lista fixa. |
| **FDP_ITC.1** — importação de dados com regras | T.MOD, T.SERVIDOR | Arquivos lidos com teto de 256 KB; no máximo 64 frames e 99 mascotes; nomes/`about`/lembretes sem caracteres de controle nem de direção de texto (Unicode bidi) e com tamanho limitado; ids de pasta só `[a-z0-9-]`. |
| **FCS_STG / FDP_RIP.2** — guarda de segredos e resíduos | T.CHAVE | Chave no Gerenciador de Credenciais (DPAPI, `CRED_PERSIST_LOCAL_MACHINE`), nunca em arquivo nem variável de ambiente; campo mascarado; buffers com a chave zerados após o uso; chave validada (sem espaços/controle — impede injeção de cabeçalho HTTP). |
| **FTP_ITC.1 / FCS_TLSC** — canal confiável | T.REDE, T.CHAVE | Só HTTPS com TLS 1.2+; `http://` apenas para `localhost`/`127.0.0.1`/`[::1]`; redirecionamentos desligados (a chave não segue para outro host); certificados validados pelo WinHTTP. Conferido no app **e** no plugin. |
| **FPT_TST / FPT_FLS** — falha segura | T.SERVIDOR | Resposta HTTP até 64 KB (uma conversa tem 1–3 KB); pedido até 64 KB; JSON com até 64 níveis (não estoura a pilha); `max_tokens` com teto de 4096; saída de qualquer plugin até 16 KB e processo encerrado após 120 s. |
| **FPT (proteção do TSF)** | T.DLL | `winhttp.dll` carregada só de System32 (`LOAD_LIBRARY_SEARCH_SYSTEM32`); as demais DLLs importadas são "KnownDLLs". Binários com ASLR (alta entropia) e DEP; manifesto `asInvoker`. Plugin personalizado só `.exe` com caminho absoluto (nada de `.bat`/PATH). |
| **FDP_ACF.1 / FPT_TST.1** — controle de acesso e integridade dos plugins | T.PLUGIN | Plugin novo chega desligado; ligar exige sua confirmação (padrão "Não") e grava o SHA-256 do programa. Antes de **cada** execução o hash é conferido; se mudou, não roda e o mascote avisa. No `config.ini`, plugin de terceiros sem hash válido é ignorado. O programa precisa ser `.exe`/`.ps1` **dentro** da pasta do plugin (sem caminhos nem `..`); o PowerShell usado é o de System32. A pergunta rotula o plugin como **acesso completo (não isolado)**, lista o que o app limita e mostra o SHA-256 que será aprovado (calculado antes da pergunta). |
| **FPT_FLS / FRU_RSA.1** — contenção dos processos filhos | T.PLUGIN, T.RECURSOS | Todo filho (plugins, `--ia`, atualização, galeria) nasce suspenso e só roda depois de entrar num *Job Object* (`src/child.rs`); se o job não puder ser criado ou aplicado, o filho é encerrado sem rodar. O job: 1 processo só (não abre outros programas), 512 MB de memória, morre com o app (`KILL_ON_JOB_CLOSE`), sem caixa de erro do Windows, e restrições de interface — sem área de transferência, sem usar janelas de fora do job (não forja mensagens para o mascote), sem mudar configurações do sistema/tela, trocar de área de trabalho ou desligar o PC. Testado em `cargo test` (processo bloqueado, stdin funcionando, plugin de exemplo). |
| **FPT_TUD_EXT / FCS_COP** — atualização confiável | T.ATUALIZACAO | Só de `https://github.com/4TyllaL/notStayAlone/releases/download/` (endereço conferido duas vezes, no app e no processo que baixa); o SHA-256 vem do campo `digest` da API do GitHub e o arquivo baixado precisa bater, começar com `MZ` e ter até 16 MB; além disso, `dontStayAlone.exe.sig` (ao lado do `.exe` na release) precisa ser uma assinatura **Ed25519** válida da frase `!StayAlone <versão> sha256:<hash>` pela chave pública embutida no app (`RELEASE_KEY`); a chave privada fica só no PC de quem publica (`examples/assinar.rs`), nunca no GitHub. Assinar a versão junto com o hash impede reaproveitar um `.exe` antigo assinado numa tag nova; nunca volta para versão mais antiga nem aceita pré-release. Com tudo conferido, o app mostra o que verificou (origem, SHA-256, assinatura, versão) e o que mudou (itens das notas da release, sem Markdown, no máximo 6 × 140 caracteres) e **só instala se você confirmar**; se não, apaga o arquivo baixado. O `.exe` antigo é renomeado (`.old.exe`) antes da troca e apagado na próxima abertura. |
| **FDP_ITC.2** — importação da galeria | T.GALERIA | Só de `raw.githubusercontent.com/4TyllaL/notStayAlone/main/gallery/`; cada arquivo tem SHA-256 no `index.json` e todos são conferidos **antes** de gravar qualquer um; ids `[a-z0-9_-]`. **Só mascotes:** os únicos arquivos aceitos são `mascot.txt`, `phrases.txt` e `phrases_en.txt` (texto lido pelos parsers com limites de T.MOD) — nada executável; um item com qualquer outro arquivo é descartado inteiro. Plugins ficam fora da galeria de propósito. No repositório, `tools/gallery.py` recusa outros arquivos e o `cargo test` confere índice, hashes e se cada mascote abre. |
| **FPR_ANO / FDP_RIP** — minimização na memória | T.MEMORIA | Desligável; só fatos curtos marcados pela IA, no máximo 30, num arquivo de texto local que você vê, edita e apaga pelas Configurações. Fatos com senha, documento, cartão, conta, e-mail ou números longos são descartados antes de gravar. |
| **FRU_RSA.1** — cotas de recursos | T.RECURSOS | Timers ligados só quando necessários; janelas, fontes e bitmaps criados sob demanda e liberados (sem vazamento GDI/USER medido); até 50 lembretes seus; histórico da conversa com 12 mensagens. |
| **FPR_UNO / FDP_IFC.1** — privacidade e fluxo | T.PRIVACIDADE | Sem hooks de teclado/mouse; só `GetLastInputInfo` (quando, nunca o quê). "Quieto em reuniões" lê só o nome do `.exe` da janela da frente, nunca o conteúdo. Fora a conversa (se você configurar), a procura de versão (desligável) e a Galeria (quando aberta), nada sai do PC; sem telemetria nem logs. |

**Transparência:** Configurações → Sobre → *Segurança e privacidade* mostra ao usuário o
estado real destas proteções (versão e commit de origem, SHA-256 do `.exe` em uso,
proteções lidas do cabeçalho PE do próprio processo — DEP, ASLR, ASLR de alta entropia, CFG —,
atualização assinada e com
confirmação, onde está a chave, serviços de rede ligados, início com o Windows, plugins que
rodam programas), inclusive o que ainda falta (sem Authenticode).

## Riscos residuais (o que ainda não está coberto)

- **Sem assinatura digital (Authenticode).** Para distribuir, assine o `.exe`: o
  Windows SmartScreen confia mais e dá para detectar adulteração.
- **Control Flow Guard só no build MSVC.** A toolchain GNU não gera CFG. O build MSVC
  (`.cargo/config.toml`) liga CFG, CRT estático, `/CETCOMPAT` e `/DEPENDENTLOADFLAG`, e
  `tools/release.ps1` para a release se faltar alguma proteção (`.github/scripts/check-exe.ps1`).
  **As releases 1.1.x a 1.2.4 saíram com a toolchain GNU, sem CFG** (DllCharacteristics
  `0x0160`: DEP e ASLR, sem `GUARD_CF`); a partir da 1.2.5 todas saem do build MSVC. O
  cartão *Segurança e privacidade* mostra as proteções lidas do cabeçalho do próprio `.exe`
  em execução.
- **Sem atestação de origem (Sigstore) por enquanto.** O workflow de release gera
  `actions/attest-build-provenance`, mas o GitHub Actions da conta está bloqueado, então as
  releases são compiladas localmente e **não há atestação publicada** — `gh attestation
  verify` não encontra nada. O que existe no lugar: a assinatura Ed25519 (chave offline) e o
  `BUILDINFO.txt` de cada release (commit, toolchain Rust/MSVC/Windows SDK fixada, flags,
  SHA-256 do `.exe` e do `Cargo.lock`). O build é reproduzível bit a bit (`/Brepro`, data
  de lançamento = data do commit, caminhos locais remapeados): qualquer pessoa pode compilar
  o mesmo commit e comparar o SHA-256. Quando o Actions
  voltar, a release volta a sair de lá com atestação.
- **Atualização depende da chave de assinatura.** Controlar o GitHub não basta: sem a
  chave Ed25519 privada, a release não é aceita. O risco passa a ser essa chave (vazar ou
  se perder: sem ela, as versões instaladas não conseguem mais se atualizar sozinhas e é
  preciso baixar a nova à mão). O primeiro download continua sem cadeia de confiança
  independente até haver Authenticode.
- **Plugins sem sandbox de arquivos e rede.** O *Job Object* impede abrir outros
  programas e usar a interface de outras janelas, mas um plugin ligado ainda lê e grava
  os seus arquivos e usa a rede com as permissões do usuário: a aprovação garante que é
  *o arquivo que você aprovou*, não que ele seja bom. Por isso a pergunta o chama de
  "acesso completo". Ligue só plugins de quem você confia. Scripts `.ps1` rodam com `-ExecutionPolicy Bypass`
  (a aprovação por hash faz o papel da política de execução).
- **Conteúdo da IA.** A resposta é texto limpo e cortado, mas não passa por filtro de
  conteúdo; o provedor escolhido é quem modera.
- **Atacante com a conta do usuário** (A.CONTA) pode ler a credencial, como qualquer
  programa do próprio usuário — é o limite do modelo de segurança do Windows.

## Como verificar

```bash
cargo test
```

Os testes cobrem, entre outros: mensagens forjadas (`mailbox`), endereços HTTP proibidos,
nomes/chaves inválidos, arquivos grandes demais, JSON profundo, limite de tokens e
limpeza de textos. Também: impressão digital de plugins, programas fora da pasta do plugin e SHA-256 (vetores oficiais), endereços de atualização e da galeria recusados, caminhos da galeria com `..`, filtro de dados sensíveis da memória e tradução completa da interface. `cargo test -- --ignored` testa o Gerenciador de Credenciais de verdade.

## Reportar um problema

Encontrou uma falha? Abra uma *issue* sem detalhes exploráveis e peça um contato privado,
ou fale diretamente com o mantenedor.
