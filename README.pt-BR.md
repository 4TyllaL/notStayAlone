# 🐾 !StayAlone

> [English](README.md) · **Português**

Um mascote em pixel art que faz companhia na área de trabalho — feito para ser
**o mais leve possível**. A interface está em **português** e **inglês** (segue o Windows, ou
escolha em Configurações).

| Métrica | Meta | Medido |
|---|---|---|
| Executável | < 5 MB | ~815 KB — um único `.exe`, conversa com IA inclusa |
| Memória privada | < 20 MB | ~2,3 MB |
| CPU | ~0% | ~0,03% da máquina (0,05–0,4% de um núcleo: parado → andando) |

## Baixar

Baixe o **[`dontStayAlone.exe`](https://github.com/4TyllaL/notStayAlone/releases/latest)** na
última versão e abra (Windows 10/11, 64 bits). Esse arquivo único é o app inteiro, conversa com IA
inclusa. Sem instalador; nada é gravado fora de `%APPDATA%\StayAlone`.

> Por que *dont*StayAlone? O app se chama **!StayAlone**, mas o GitHub tira o `!` do nome dos arquivos.

O executável ainda não tem assinatura digital, então o SmartScreen pode avisar na primeira vez
(*Mais informações → Executar assim mesmo*). O GitHub mostra o SHA-256 do arquivo ao lado do
download.

Na primeira vez, uma janela de boas-vindas ajuda a escolher o mascote, os lembretes e
(opcional) a chave da IA. Depois o app **se atualiza sozinho**: uma vez por dia olha a última
versão no GitHub e o painel oferece a nova. O download é conferido pelo SHA-256 da release e por
uma assinatura Ed25519, feita com uma chave que nunca sai do PC de quem publica, antes
de trocar o `.exe`; aí ele mostra o que conferiu e o que mudou, e só instala se você disser sim
(dá para desligar a procura em Configurações).

## Configurações

Painel → **Configurações** (ou `dontStayAlone.exe --configurar`). A janela tem uma barra lateral
com o seu mascote (muda na hora quando você escolhe outro) e as páginas:

- **Geral:** mascote, **amigo na tela** (um segundo mascote que passeia e visita o
  primeiro), tamanho, velocidade, tempo para considerar que você saiu, seu aniversário,
  iniciar com o Windows, procurar versões novas, ficar quieto em reuniões, tema (claro,
  escuro ou igual ao Windows), idioma e acessórios de época.
- **Lembretes:** os três embutidos e quantos lembretes seus quiser; a **meta de água**
  (copos por dia) e o **foco** (minutos de foco e de pausa, e se os lembretes esperam a pausa).
- **Conversa:** serviço de IA, modelo e a **chave da API** (fica no Gerenciador de
  Credenciais do Windows, protegida pela sua conta — nunca em arquivo), com botão de teste.
  E a **memória**: ligada, o mascote lembra do que você conta (uma prova na sexta, o nome
  do seu gato) e puxa o assunto depois. Fica só num `memoria.txt` no seu PC, que você vê,
  edita ou apaga ali mesmo; senhas, documentos e números nunca são guardados.
- **Criar mascote:** desenhe o seu mascote (ou comece a partir de um existente) e o app
  cria sozinho as animações de piscar, dormir, andar e pular. Se quiser, desenhe também
  as poses dormindo, comendo, feliz e andando (a pose parada aparece clarinha por baixo).
  Mascotes podem ser 16×16 ou 32×32 — ou descreva o mascote e peça para a **IA desenhar**,
  depois ajuste. "Salvar e usar" já coloca ele na tela (fica em `%APPDATA%\StayAlone\mascots\`).
- **Plugins:** liga e desliga os plugins (veja abaixo), com botão de teste.
- **Sobre** (no rodapé da barra lateral): o que é o app, a versão, a data de lançamento e
  links para a minha página de projetos e para este repositório. O cartão **Segurança e
  privacidade** mostra, sem jargão, o que protege você: SHA-256 do `.exe` em uso (compare
  com o do GitHub), como as atualizações são conferidas, onde a chave da API fica, com quais
  serviços o app fala, se ele inicia com o Windows e quantos plugins podem rodar programas.
- **Galeria:** mascotes da comunidade, instalados com um clique. Só mascotes — desenho e
  falas em texto, nada que rode no seu PC. Os arquivos vêm da pasta [`gallery/`](gallery)
  deste repositório e são conferidos (SHA-256) antes de gravar. Para publicar o seu, mande
  um pull request com a pasta em `gallery/mascots/<id>/` (`mascot.txt` e, se quiser,
  `phrases.txt` e `phrases_en.txt`) e rode `python tools/gallery.py`; o `cargo test` confere
  se o mascote abre e se o índice está em dia. Plugins não entram na galeria.

Mascotes podem ser "ele" ou "ela" (`article o|a` no `mascot.txt`): as falas se ajustam.

## Conversar com o mascote (opcional)

Botão direito → **Conversar com ...** (ou `--conversar`): uma caixinha aparece
acima dele; Enter envia, Esc fecha. Ele responde no balão, com a personalidade
dele e lembrando das últimas mensagens.

Quem responde é o plugin de conversa **nativo**, que fala com qualquer API no **padrão
OpenAI** (`/chat/completions`). O padrão é o **Gemini**:

1. Crie uma chave em <https://aistudio.google.com/apikey>.
2. Cole em **Configurações → Conversa**. Ela vai para o Gerenciador de Credenciais do
   Windows (como `StayAlone:GEMINI_API_KEY`) e nunca para arquivo nenhum. Quem preferir
   pode usar uma variável de ambiente com o mesmo nome:

```bash
setx GEMINI_API_KEY "sua-chave"
```

Troque de provedor (OpenAI, OpenRouter, Ollama local...) na mesma aba; fica salvo em
`%APPDATA%\StayAlone\chat.ini` (só `api_base`, `model` e `api_key_env`, com exemplos).
A chave só viaja por **HTTPS** (TLS 1.2+); `http://` é aceito apenas para serviços no
seu próprio PC, como o Ollama. O processo do mascote nunca usa a rede: ao conversar, o app
abre uma segunda cópia de si mesmo (`dontStayAlone.exe --ia`), que faz o pedido e fecha.

## Plugins

Plugins são programas (`.exe` ou script PowerShell `.ps1`) que dão novos poderes ao
mascote. Cada um é uma pasta em `%APPDATA%\StayAlone\plugins\` com um `plugin.ini`, e
aparece na aba **Configurações → Plugins**, onde você liga e desliga. Dois tipos:

- **avisos** — roda de tempos em tempos (`every = 90` minutos) e o mascote fala o que ele
  escrever. Ex.: clima, agenda, cotação, status de um build. Só roda com você no PC e
  fora do "Silenciar".
- **conversa** — responde quando você conversa com o mascote. O nativo (IA) é um deles;
  só um fica ligado por vez.

```
name  = Curiosidades
about = De vez em quando o mascote conta uma curiosidade.
kind  = avisos
run   = curiosidades.ps1
every = 90
```

O app manda um JSON no stdin e lê a resposta no stdout — o protocolo completo está no
`LEIA-ME.txt` da pasta, junto com o exemplo **Curiosidades** (já vem, desligado).

**Segurança:** plugin novo chega desligado; para ligar, você confirma e o app guarda a
impressão digital (SHA-256) do programa. Se o arquivo mudar, o plugin é pausado até você
ligar de novo. A pergunta diz com todas as letras que é um plugin de **acesso completo**
(roda com as permissões da sua conta) e mostra o SHA-256. O programa precisa estar dentro
da pasta do plugin, cada execução tem no máximo 2 minutos, 16 KB de resposta e 512 MB de
memória, e o aviso falado tem até 200 caracteres. Cada execução roda num *Job Object* do
Windows: sem abrir outros programas, sem área de transferência, sem mexer nas suas janelas
ou nas configurações do sistema, e encerrada se o app fechar.

## Os mascotes

| | | |
|---|---|---|
| **Calcifer** | gatinho laranja | come peixinho |
| **Lance** | cachorrinho caramelo | come osso |
| **Zezé** | coelhinho loirinho | come cenoura |
| **Jujubs** | dinossaura verde de laço rosa | come jujubas |

Troque clicando no desenho dele no painel. Cada um tem falas próprias ("Miau!", "Au au!",
"\*mexe o narizinho\*") por cima das falas padrão.

## O que ele faz

**O mascote**
- Anda em cima da barra de tarefas, pisca, boceja e cochila (mais cedo à noite).
- **Clique** = carinho (ou acorda, se estiver dormindo). Às vezes ele responde.
- **Arraste** para pegá-lo; solte para ele cair — dá até para arremessar.
- Some sozinho quando há um jogo/vídeo/apresentação em tela cheia.

**Brincar** (atalhos do painel)
- **Dar petisco:** a comida favorita cai perto dele; ele corre, come e agradece.
  No máximo um a cada 20 min — antes disso ele diz que está cheio.
- **Jogar bolinha:** ele corre atrás e chuta. Você também pode arrastar e arremessar
  a bolinha, ou clicar nela para dar um tapinha. Depois de um tempo ele cansa.

**A companhia**
- Diz bom dia / boa tarde / boa noite quando o app abre.
- Percebe quando você sai do PC (5 min sem teclado/mouse): ele dorme. Quando você
  volta, acorda feliz e comemora — isso conta como uma pausa no dia.
- Lembretes gentis, contados só em tempo de uso: beber água (45 min),
  alongar (60 min) e descansar os olhos (20 min, desligado por padrão).
  Clique no balão para confirmar ("bebi água!"), ou use **Bebi água** no painel
  (ou `--agua`) sempre que beber — conta no resumo do dia. Voltar de uma pausa zera o de alongar.
- **Pausa guiada:** no lembrete de olhos ou de alongar, clique no balão e o mascote conduz a
  pausa com você, passo a passo, com contagem ("Olhe para algo bem longe... 18").
- **Meta de água** (8 copos por padrão): barrinha de progresso no painel; ele comemora quando
  você bate a meta e conta os dias seguidos. Na segunda de manhã, resume a semana que passou
  (tempo juntos, pausas e água). O histórico fica só no `state.ini`, no seu PC.
- Nunca vira metralhadora: no mínimo 2 min entre lembretes; silenciado ou escondido,
  os lembretes são descartados (não acumulam); em tela cheia, esperam.
- Depois da meia-noite, sugere descansar (no máximo 1× por hora).
- **Bateria** (notebooks): abaixo de 20% ele avisa e fica cansado (anda devagar,
  boceja mais); agradece quando você liga o carregador.
- **Digitação intensa:** quando você digita sem parar por uns 20 s, ele comemora
  (e às vezes comenta, no máximo a cada 45 min).
- **Afeto** (♥ na dica do ícone): sobe com carinho, pausas, água, petiscos e
  brincadeiras; cai bem devagar e nunca abaixo de um piso.
- **Resumo do dia** (automático após as 18h, ou pelo painel): tempo juntos, pausas e água.
- **Foco** (pomodoro, 25/5 min por padrão, ajustável) pelo painel; durante o foco os
  lembretes esperam a pausa.
- **Rotina:** parabéns no seu aniversário, falas de segunda e de sexta, um toque depois de
  3 horas sem pausa, e fica **quieto em reuniões** (Teams, Zoom, Webex... — ele olha só o
  nome do programa na frente, nunca a tela).
- **O amigo na tela** visita o principal: se ele estiver dormindo, cochila junto; acordados,
  comemoram e às vezes se cumprimentam; e dividem o petisco.
- **Acessórios de época:** gorro no Natal, chapéu de bruxa no Halloween, chapéu de palha na
  festa junina e chapéu de festa no seu aniversário e no Ano-Novo.
- **Ctrl+Alt+M** abre a conversa de qualquer lugar (o app registra só essa combinação e nunca
  lê o teclado; dá para desligar em Configurações → Conversa).

**Privacidade:** nada vai para a internet (a não ser a conversa com a IA, se você
configurar). Ele nunca lê o teclado — sabe só *se* houve atividade (`GetLastInputInfo`).
A "digitação intensa" é deduzida de atividade com o cursor parado; nenhuma tecla é
registrada. Detalhes de segurança em [`SECURITY.md`](SECURITY.md).

**Painel** — botão direito no mascote, ou clique no ícone da bandeja:

- mascote, corações e o resumo de hoje;
- atalhos grandes: **Petisco**, **Bolinha**, **Conversar** e **Foco**;
- trocar de mascote clicando no desenho (o **+** abre o criador de mascotes);
- tamanho num seletor (Mini, P, M, G) e interruptores para silenciar e esconder;
- Bebi água (com a barrinha da meta), Lembretes, Resumo do dia, Configurações e Sair;
- uma faixa "Versão nova disponível" quando há atualização.

**Linha de comando** — útil para atalhos do Windows; funciona com o app já aberto:

```bash
dontStayAlone.exe --bolinha
```

Também: `--petisco`, `--agua`, `--conversar`, `--resumo`, `--configurar` e `--esconder`
(alterna esconder/mostrar).

## Compilar

Requer Rust (toolchain `stable-x86_64-pc-windows-gnu` ou MSVC). Com MSVC, o
`.cargo/config.toml` liga o Control Flow Guard e o CRT estático. As releases saem do
`tools/release.ps1`: toolchain MSVC fixada (Rust 1.98.1), árvore git limpa, conferência de
DEP, ASLR, ASLR de alta entropia e CFG, assinatura Ed25519 da atualização e um
`BUILDINFO.txt` publicado ao lado do `.exe` com o commit exato, as versões de Rust, Cargo,
MSVC e Windows SDK, as flags e os hashes. O build é reproduzível bit a bit (sem data/hora
nem caminhos da máquina no `.exe`): o mesmo commit com a mesma toolchain, MSVC e SDK dá o
mesmo SHA-256. As releases até a 1.2.4 foram compiladas com a toolchain GNU e não têm CFG.

```bash
cargo build --release
```

Gera `target/release/dontStayAlone.exe` (um único arquivo, com os quatro mascotes, a conversa
com IA, o ícone e o manifesto embutidos). O ícone é desenhado na compilação a
partir do sprite do Calcifer (`build.rs`), sem ferramentas externas.

```bash
cargo test
```

O `tools/smoke.ps1` é um teste de tela: abre o app de verdade numa pasta de dados isolada,
passa pelas boas-vindas, pelo painel, pelas Configurações (em português e em inglês), por uma
pausa guiada e pelo atalho da conversa, e confere o que foi gravado. Ele se recusa a rodar com
o app aberto e só fotografa as janelas do app; com `-Docs`, atualiza as imagens em `docs/`.

### Publicar uma versão

O app só aceita uma atualização assinada com a chave Ed25519 de quem publica (a pública
está em `RELEASE_KEY`, `src/update.rs`; a privada em `%USERPROFILE%\.stayalone\release-key.txt`,
fora do repositório — guarde uma cópia). Para cada release, anexe o `.sig` junto do `.exe`:

```bash
cargo run --release --example assinar -- 1.2.3 target/release/dontStayAlone.exe
```

```bash
cargo test --release -- --ignored release_is_signed
```

```bash
gh release create v1.2.3 target/release/dontStayAlone.exe target/release/dontStayAlone.exe.sig --notes-file docs/releases/v1.2.3.md
```

## Personalizar e criar mascotes (mods)

- **Configurações:** `%APPDATA%\StayAlone\config.ini` — mascote, tamanho, velocidade,
  minutos para considerar que você saiu e intervalo de cada lembrete.
  O afeto e as estatísticas do dia ficam em `state.ini`, na mesma pasta.
- **Novos mascotes:** Configurações → Geral → **Abrir pasta de mascotes** abre
  `%APPDATA%\StayAlone\mascots\` (com um LEIA-ME e os mascotes embutidos como modelo).
  Cada pasta com um `mascot.txt` vira um mascote no menu; um `phrases.txt` opcional
  muda só os tópicos de fala que definir. Uma pasta `calcifer`, `lance`, `zeze` ou
  `jujubs` substitui o mascote embutido. Também vale uma pasta `mascots\` ao lado do `.exe`.
  Arquivos de mods com mais de 256 KB são ignorados; nomes e textos são limpos antes de
  aparecer em menus, balões ou na conversa.
- **Idiomas:** as falas em inglês ficam em `phrases_en.txt` (padrão e na pasta de cada
  mascote). Em inglês, um mascote sem `phrases_en.txt` usa as falas padrão em inglês.
- **Formato dos sprites:** texto puro, 16×16 (ou 32×32 com a linha `size 32`), uma letra por cor — veja
  [`assets/mascots/calcifer/mascot.txt`](assets/mascots/calcifer/mascot.txt).
  `idle2`, `look`, `eat1`, `eat2` e `food` são opcionais; `about` descreve a personalidade
  usada na conversa.
- **Falas padrão:** [`assets/phrases.txt`](assets/phrases.txt), agrupadas por `[tópico]`.
  Um `phrases.txt` ao lado do `.exe` substitui as falas padrão sem recompilar.

## Como é leve

- Janelas Win32 nativas em camadas (`UpdateLayeredWindow`), sem WebView nem framework de UI.
- Só redesenha quando o frame muda; cada timer liga só quando precisa
  (60 fps só durante quedas e bolinha em movimento, 10 fps normal, ~1,4 fps dormindo,
  tudo parado quando escondido).
- Balão e objetos só ocupam memória enquanto estão na tela.
- Sem hooks de teclado/mouse globais; rede só no processo `--ia`, e só quando você conversa.
- Janelas de configurações e conversa, fontes e bitmaps existem só enquanto estão abertos
  (sem vazamento de objetos GDI/USER, conferido abrindo e fechando repetidas vezes).
- Enquanto você digita, ele não se move nem abre balões: mover janelas faz o Windows
  reexibir o ponteiro escondido ("ocultar ponteiro ao digitar").

## Estrutura

```
src/main.rs            janela do mascote, mouse, menu e timers — liga tudo
src/mascot.rs          máquina de estados e física do mascote (sem Win32, testável)
src/prop.rs            física da bolinha e do petisco (sem Win32, testável)
src/companion.rs       presença, lembretes, afeto, bateria, digitação (sem Win32, testável)
src/maker.rs           criador de mascotes: 1 desenho → todas as poses (testável)
src/sprite.rs          parser e desenho dos sprites ASCII
src/phrases.rs         parser, sobreposição e sorteio das falas
src/pack.rs            mascotes embutidos e mods
src/config.rs          config.ini, state.ini, chat.ini e "Iniciar com o Windows"
src/plugins.rs         plugins: descoberta, aprovação por SHA-256, execução com limites
src/sha256.rs          SHA-256 (plugins, atualizações e galeria)
src/lang.rs            idioma da interface (português no código, tabela em inglês)
src/update.rs          atualização automática pelas releases do GitHub (SHA-256 + Ed25519)
src/gallery.rs         galeria da comunidade
src/memory.rs          memória da conversa (memoria.txt)
src/buddy.rs           o amigo na tela
src/guide.rs           pausa guiada (passos e contagem, testável)
src/accessory.rs       acessórios de época (chapéus em pixel art)
tools/smoke.ps1        teste de tela numa pasta de dados isolada
examples/assinar.rs    assina as releases (chave privada fora do repositório)
src/welcome.rs         boas-vindas da primeira vez
src/net.rs             HTTPS via WinHTTP (só nos processos filhos)
src/child.rs           processos filhos com limite de tempo e de saída
src/ui.rs              botões, interruptores e tema escuro dos controles
src/secret.rs          chave da API no Gerenciador de Credenciais do Windows
src/mailbox.rs         dados entre janelas/threads sem ponteiros nas mensagens
src/system.rs          leituras do sistema (tempo ocioso, bateria, monitores, tela cheia)
src/tray.rs            ícone da bandeja
src/flyout.rs          painel do mascote (clique direito / bandeja), desenhado à mão
src/theme.rs           cores, fontes e ícones da interface
src/bubble.rs          balão de fala (janela em camadas + texto GDI)
src/chat.rs            caixinha de conversa + chamada ao plugin
src/gfx.rs             superfície de desenho compartilhada
src/win.rs             utilitários Win32 (texto, fontes, limpeza de strings)
src/settings/mod.rs    janela de configurações (barra lateral, cartões, botões)
src/settings/editor.rs editor de pixels e paleta da aba "Criar mascote"
build.rs               ícone, manifesto e versão do .exe
src/ai/                conversa com IA (API padrão OpenAI via WinHTTP, JSON mínimo), roda como --ia
assets/                mascotes, objetos, falas e o plugin de exemplo
```
