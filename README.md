# 🦀 Rust Virtual Desktop Helper (`rust-vd-helper`)

Uma reescrita minimalista, ultra-leve e didática em **Rust** do utilitário *WindowsVirtualDesktopHelper*, focada exclusivamente em **monitorar a Área de Trabalho Virtual ativa no Windows 10/11 e exibir seu número na Bandeja do Sistema (System Tray)** — sem janelas fantasmas poluindo o seu **Alt+Tab**.

## Licença e créditos

Este projeto está licenciado sob a **MIT License**. Consulte o arquivo [`LICENSE`](LICENSE) para ver os termos completos. O aviso de copyright e o texto da licença devem ser mantidos em cópias ou partes substanciais do software.

O projeto original e referências usados durante o desenvolvimento podem ter seus próprios autores e termos de licença. Os créditos e avisos de licença aplicáveis a código ou materiais derivados devem ser preservados; a licença MIT deste repositório não substitui nem altera as licenças de dependências ou de componentes de terceiros.

---

## 🎯 Por que este projeto foi criado?

Na versão original em **C# (.NET / Windows Forms)**:

1. **Bug do Alt+Tab:** O aplicativo precisava instanciar formulários (`AppForm`, `OverlayForm`, `SwitchNotificationForm`) usando estilos de janela (`WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT`) que **não ocultam** a janela do gerenciador *Alt+Tab* do Windows (faltava `WS_EX_TOOLWINDOW` ou uma janela *Message-Only*).

2. **Vazamento de Recursos (GDI Handle Leak):** Cada troca de área de trabalho gerava bitmaps GDI/HICON que podiam acumular na memória.

3. **Peso desnecessário:** Para quem deseja apenas ver o número do desktop ativo na bandeja do relógio, carregar todo o runtime do .NET e múltiplas janelas de overlay é desnecessário.

### ✨ O que esta versão em Rust faz diferente?
- **Zero presença no Alt+Tab:** Usa `#![windows_subsystem = "windows"]` e um loop de mensagens Win32 nativo (`GetMessageW` + `SetTimer`) sem criar **nenhuma janela de aplicativo**.

- **Customização total via `config.toml`:** Altere a **fonte** (`.ttf`), o **tamanho da fonte**, a **cor do número** e a **cor de fundo** a qualquer momento e recarregue pelo próprio ícone da bandeja sem fechar o programa.

- **Consumo ínfimo:** Ocupa ~2 a 4 MB de RAM, 0.0% de CPU em repouso (graças ao cache de ícones em memória) e gera um executável único pequeno.

---

## 📂 Estrutura Didática do Projeto

```text
RUST/
├── .cargo/
│   └── config.toml        # Força linkagem 100% estática (+crt-static, sem depender de vcruntime140.dll)
├── Cargo.toml             # Dependências do projeto e otimizações de compilação
├── config.toml            # Arquivo de configuração (Fonte, Tamanho, Cores)
├── README.md              # Documentação e guia de estudo
└── src/
    ├── main.rs            # Ponto de entrada, System Tray, detecção e Loop Win32
    ├── vd_win10.rs        # Implementação COM direta (`#[repr(C)]`) para Windows 10 (Build 19045)
    ├── config.rs          # Leitura e parser seguro do `config.toml` usando `serde`
    └── icon_renderer.rs   # Desenho do número em buffer RGBA 64x64 usando `ab_glyph`
```

---

## 🛠️ Como Compilar e Executar

Certifique-se de ter o [Rust instalado](https://rustup.rs/) no Windows.

1. Abra o terminal na pasta `RUST`:
   ```powershell
   cd D:\Workspace\DEV\WindowsVirtualDesktopHelper\RUST
   ```

2. Para testar em **modo de desenvolvimento** (mostra mensagens de erro no terminal se houver):
   ```powershell
   cargo run
   ```

3. Para gerar o **executável final otimizado** (sem janela de terminal, pronto para uso diário):
   ```powershell
   cargo build --release
   ```
   O executável final estará em:
   `D:\Workspace\DEV\WindowsVirtualDesktopHelper\RUST\target\release\rust-vd-helper.exe`
   *(Dica: mantenha o arquivo `config.toml` na mesma pasta do `.exe`, ou deixe que o programa crie um automaticamente na primeira execução).* 

---

## ⚙️ Como Configurar (`config.toml`)

Você pode clicar com o **botão direito** no ícone da bandeja do sistema e escolher:
- **"Abrir config.toml no Bloco de Notas"** para editar.
- **"Recarregar Configuração (config.toml)"** para aplicar suas mudanças na hora!

Exemplo de `config.toml`:

```toml
# Nome do arquivo da fonte em C:\Windows\Fonts\ ou caminho completo
font_file = "seguisb.ttf"

# Tamanho da fonte em pixels (dentro do ícone 64x64)
font_size = 54.0

# Cor da fonte em Hexadecimal (#RRGGBB ou #RRGGBBAA)
font_color = "#FFFFFF"

# Cor de fundo do ícone (#00000000 = 100% transparente)
background_color = "#00000000"

# Ajuste fino de posição horizontal e vertical (em pixels)
offset_x = 0
offset_y = 0

# Intervalo de verificação em milissegundos
poll_interval_ms = 200
```

### 🎨 Fontes populares do Windows para testar no `font_file`:
| Aparência | Nome do Arquivo em `C:\Windows\Fonts` |
| :--- | :--- |
| **Segoe UI Semibold** *(Padrão)* | `seguisb.ttf` |
| **Segoe UI Bold** *(Mais grossa)* | `segoeuib.ttf` |
| **Arial Bold** *(Clássica)* | `arialbd.ttf` |
| **Consolas Bold** *(Estilo código/terminal)* | `consolab.ttf` |
| **Bahnschrift** *(Geométrica moderna)* | `bahnschrift.ttf` |

---

## 🧠 Lições aprendidas: decisões e diagnóstico

A rotina de detecção foi construída por etapas. O resultado que funcionou no Windows 10 22H2 (Build 19045) foi consultar o Registro para obter o GUID do desktop ativo e compará-lo com a lista ordenada de GUIDs dos desktops. Essa experiência mostrou que, para uma API interna e não documentada do Windows, uma implementação COM plausível não basta: é essencial validar os dados no sistema real e confirmar que eles mudam ao trocar de desktop.

### Objetivo e decisões iniciais

O objetivo desta versão Rust era manter o escopo enxuto: detectar a Área de Trabalho Virtual ativa, mostrar seu número no ícone da bandeja e permitir personalização pelo `config.toml`. A implementação original em C# serviu como referência para entender a lógica e a ordem dos desktops; a crate `winvd` também foi considerada para consultar as APIs de desktops virtuais.

Como o ambiente de teste é Windows 10 22H2 (Build 19045), foi criada uma rotina COM própria em `src/vd_win10.rs`, modelando interfaces por meio de `#[repr(C)]` e chamadas às vtables. A intenção era reproduzir a abordagem usada pelo código C# de referência e manter `winvd` como alternativa. No entanto, a rotina COM retornava sempre o desktop 1 no ambiente de teste. A hipótese de incompatibilidade ou mudança nas interfaces internas era possível, mas não foi demonstrada como causa; por isso, não se deve tratar essa explicação como conclusão comprovada.

### Diagnóstico que levou à solução

Em vez de continuar presumindo que a falha estava em uma assinatura COM específica, inspecionamos os valores existentes no Registro do usuário. Encontramos:

- `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops\VirtualDesktopIDs`: sequência binária com os GUIDs dos desktops, na ordem usada para determinar o número exibido.
- `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SessionInfo\1\VirtualDesktops\CurrentVirtualDesktop`: GUID binário do desktop ativo na sessão observada.

Os dados fornecidos durante o diagnóstico mostraram que o GUID de `CurrentVirtualDesktop` correspondia a um dos GUIDs da lista `VirtualDesktopIDs`. A solução foi, então, ler os GUIDs binários do Registro, localizar o GUID atual na lista e converter sua posição de índice iniciado em zero para o número apresentado ao usuário, iniciado em um.

### Decisão final da rotina

A implementação atual em `src/vd_win10.rs` tenta primeiro a leitura do Registro: obtém `VirtualDesktopIDs`, procura o `CurrentVirtualDesktop` — primeiro na chave direta e depois nas subchaves de `SessionInfo` — e encontra sua posição na lista. Se localizar a correspondência, usa o nome personalizado do desktop quando disponível; caso contrário, exibe `Área de Trabalho N`.

Se a consulta ao Registro não retornar uma correspondência, o código mantém caminhos alternativos: a consulta COM implementada no módulo e, por fim, `winvd`. Esses fallbacks permanecem como alternativas, mas o sucesso observado no ambiente de teste veio da comparação dos dados do Registro. A leitura foi testada pelo usuário: após a alteração, o aplicativo passou a identificar o desktop atual, e a recarga das configurações também foi confirmada como funcional.

> **Limites do que foi confirmado:** o teste bem-sucedido comprova o comportamento observado naquela instalação do Windows 10 22H2 e na sessão testada. Como as APIs COM e os detalhes do Registro usados para desktops virtuais não são uma interface pública estável, outras builds, sessões e versões do Windows devem ser testadas antes de afirmar compatibilidade universal. A rotina depende dos valores e da estrutura do Registro encontrados no sistema; mudanças do Windows podem exigir ajustes.

### Outros aprendizados do projeto

1. **Investigar antes de aprofundar o FFI (`src/vd_win10.rs`):** comparar os GUIDs disponíveis no Registro ajudou a substituir uma hipótese sobre COM por uma verificação observável. Em interoperabilidade com APIs internas, é útil separar o que foi medido do que é apenas uma explicação possível.

2. **Configuração recarregável (`src/config.rs` e `src/main.rs`):** o usuário confirmou que editar e recarregar o `config.toml` funcionou como esperado. Isso permite ajustar fonte, tamanho e cores sem reiniciar o aplicativo.

3. **Renderização sem desenhar texto via GDI (`src/icon_renderer.rs`):** `ab_glyph` rasteriza a fonte em um buffer RGBA usado na criação do ícone.

4. **Integração nativa com a bandeja (`src/main.rs`):** o projeto combina `tray-icon` com APIs Win32 para atualizar o ícone e processar o ciclo de eventos.

### Observação sobre manutenção

A solução principal de detecção já foi confirmada em uso pelo usuário, mas isso não equivale a uma garantia para todas as versões do Windows. Se a detecção falhar após uma atualização, o primeiro diagnóstico recomendado é comparar novamente `VirtualDesktopIDs` e `CurrentVirtualDesktop` antes e depois de trocar de desktop; em seguida, verificar se o GUID atual está presente na lista e se a ordem corresponde ao número esperado. Evite atribuir a falha a uma causa específica de COM ou do Windows sem evidências reproduzíveis.
