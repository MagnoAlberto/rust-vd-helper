// ============================================================================
// PROJETO: Rust Virtual Desktop Helper (`rust-vd-helper`)
// ARQUIVO PRINCIPAL: src/main.rs
//
// DIFERENCIAL CRÍTICO DESTE PROJETO (FIM DO BUG DO ALT+TAB):
// 1. A diretiva `#![windows_subsystem = "windows"]` abaixo instrui o linker do
//    Windows a NÃO abrir uma janela de console (`cmd.exe`) ao iniciar o `.exe`.
// 2. Diferente do projeto original em C# (que criava formulários `Form` do
//    Windows Forms como `AppForm`, `OverlayForm` e `SwitchNotificationForm` sem
//    o estilo `WS_EX_TOOLWINDOW`, fazendo o DWM exibi-los no Alt+Tab), aqui
//    nós NÃO criamos nenhuma janela de aplicativo! Rodamos apenas um Message
//    Loop Win32 puro (`GetMessageW` + `SetTimer`) que alimenta o System Tray.
// ============================================================================

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod icon_renderer;
mod vd_win10;

use config::AppConfig;
use icon_renderer::IconRenderer;
use std::ptr;
use std::sync::mpsc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, PostQuitMessage, SetTimer, TranslateMessage, MSG,
    WM_TIMER,
};
use winvd::DesktopEvent;

/// Estrutura que guarda o estado atual da nossa aplicação em memória.
pub struct AppState {
    tray_icon: TrayIcon,
    renderer: IconRenderer,
    last_desktop_index: Option<u32>,
    last_desktop_name: String,
}

impl AppState {
    /// Consulta a API interna do Windows (via `vd_win10` com fallback para `winvd`)
    /// para descobrir qual é a Área de Trabalho Virtual ativa no momento e atualiza o ícone.
    pub fn check_and_update_desktop(&mut self, force_redraw: bool) {
        let (display_number, desktop_name) = match vd_win10::get_active_desktop_info() {
            Some(info) => info,
            None => {
                // Mesmo se a API COM falhar temporariamente (ex: Explorer reiniciando),
                // se `force_redraw` for `true` (ex: usuário clicou em Recarregar Configuração),
                // redesenhamos o último número conhecido com a nova cor/fonte!
                if force_redraw {
                    let fallback_num = self.last_desktop_index.unwrap_or(1);
                    let num_str = fallback_num.to_string();
                    if let Ok(new_icon) = self.renderer.render_text_icon(&num_str) {
                        let _ = self.tray_icon.set_icon(Some(new_icon));
                    }
                }
                return;
            }
        };

        // Só redesenha o ícone no Windows se você realmente trocou de Desktop
        // (ou se clicou em "Recarregar Configuração").
        let changed = self.last_desktop_index != Some(display_number)
            || self.last_desktop_name != desktop_name
            || force_redraw;

        if changed {
            let number_str = display_number.to_string();

            if let Ok(new_icon) = self.renderer.render_text_icon(&number_str) {
                let _ = self.tray_icon.set_icon(Some(new_icon));
            }

            // Atualiza o Tooltip (texto que aparece ao passar o mouse sobre o ícone na bandeja)
            let tooltip = format!("Desktop {}: {}", display_number, desktop_name);
            let _ = self.tray_icon.set_tooltip(Some(&tooltip));

            self.last_desktop_index = Some(display_number);
            self.last_desktop_name = desktop_name;
        }
    }

    /// Recarrega o arquivo `config.toml` do disco em tempo real e atualiza o ícone.
    pub fn reload_configuration(&mut self) -> u64 {
        let new_cfg = AppConfig::load();
        let poll_ms = new_cfg.poll_interval_ms.max(50);
        if let Err(err) = self.renderer.reload_config(new_cfg) {
            eprintln!("Erro ao recarregar config: {}", err);
        }
        self.check_and_update_desktop(true);
        poll_ms
    }
}

fn main() {
    // 1. Carrega a configuração do arquivo `config.toml`
    let config = AppConfig::load();
    let poll_interval_ms = config.poll_interval_ms.max(50);

    // 2. Inicializa o renderizador de fontes (.ttf -> Ícone RGBA)
    let mut renderer =
        IconRenderer::new(config).expect("Falha ao inicializar o renderizador de ícones");

    // 3. Cria o menu de contexto (clique direito no ícone da bandeja)
    let tray_menu = Menu::new();
    let item_reload = MenuItem::new("Recarregar Configuração (config.toml)", true, None);
    let item_open_cfg = MenuItem::new("Abrir config.toml no Bloco de Notas", true, None);
    let item_quit = MenuItem::new("Sair", true, None);

    let _ = tray_menu.append(&item_reload);
    let _ = tray_menu.append(&item_open_cfg);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&item_quit);

    let reload_id = item_reload.id().clone();
    let open_cfg_id = item_open_cfg.id().clone();
    let quit_id = item_quit.id().clone();

    // 4. Gera o ícone inicial ("1") e cria o ícone na Bandeja do Sistema (System Tray)
    let initial_icon = renderer
        .render_text_icon("1")
        .expect("Falha ao gerar ícone inicial");

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Rust Virtual Desktop Helper")
        .with_icon(initial_icon)
        .build()
        .expect("Falha ao criar o ícone na bandeja do sistema");

    let mut app_state = AppState {
        tray_icon,
        renderer,
        last_desktop_index: Some(1),
        last_desktop_name: String::new(),
    };

    // Atualiza imediatamente para o Desktop atual no momento em que o programa abre
    app_state.check_and_update_desktop(true);

    // 5. Canal MPSC opcional para eventos COM do Windows 11 (no Windows 10 o Timer
    //    de 200ms faz a detecção leve e instantânea via `vd_win10`).
    let (vd_tx, vd_rx) = mpsc::channel::<DesktopEvent>();
    let _vd_thread = winvd::listen_desktop_events(vd_tx);

    // ========================================================================
    // 6. LOOP DE MENSAGENS NATIVO DO WINDOWS (WIN32 MESSAGE PUMP)
    // ========================================================================
    unsafe {
        let mut timer_id = SetTimer(ptr::null_mut(), 0, poll_interval_ms as u32, None);
        let mut msg: MSG = std::mem::zeroed();

        while GetMessageW(&mut msg, ptr::null_mut() as HWND, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);

            // Verifica se o usuário clicou em algum item do menu da bandeja
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == reload_id {
                    let new_poll = app_state.reload_configuration();
                    if timer_id != 0 {
                        KillTimer(ptr::null_mut(), timer_id);
                    }
                    timer_id = SetTimer(ptr::null_mut(), 0, new_poll as u32, None);
                } else if event.id == open_cfg_id {
                    // Abre o config.toml no Notepad para você editar cor/fonte facilmente
                    let cfg_path = AppConfig::config_path();
                    let _ = std::process::Command::new("notepad.exe")
                        .arg(cfg_path)
                        .spawn();
                } else if event.id == quit_id {
                    if timer_id != 0 {
                        KillTimer(ptr::null_mut(), timer_id);
                    }
                    PostQuitMessage(0);
                }
            }

            // Drena eventuais notificações COM de troca de desktop recebidas pelo canal `vd_rx`
            let mut com_event_fired = false;
            while vd_rx.try_recv().is_ok() {
                com_event_fired = true;
            }

            // A cada batida do Timer (`WM_TIMER`) ou evento COM, checamos se o Desktop mudou
            if msg.message == WM_TIMER || com_event_fired {
                app_state.check_and_update_desktop(false);
            }
        }
    }
}
