// ============================================================================
// MÓDULO: config.rs
// OBJETIVO: Carregar, validar e converter o arquivo `config.toml`.
//
// CONCEITOS DE RUST PARA ESTUDO NESTE ARQUIVO:
// 1. `#[derive(Deserialize)]`: Macro da crate `serde` que gera automaticamente
//    o código para ler um arquivo TOML e transformar na struct `AppConfig`.
// 2. `#[serde(default)]`: Caso o usuário esqueça algum campo no `config.toml`,
//    o Rust usa o valor padrão definido na trait `Default` sem dar crash.
// 3. `Result<T, E>` e `Option<T>`: Tratamento seguro de erros (sem Exceptions).
// ============================================================================

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Estrutura que representa os dados do arquivo `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Nome do arquivo de fonte (ex: "seguisb.ttf") ou caminho completo
    pub font_file: String,
    /// Tamanho da fonte em pixels (dentro do ícone 64x64)
    pub font_size: f32,
    /// Cor do texto em formato Hexadecimal (ex: "#FFFFFF" ou "#00FFCCFF")
    pub font_color: String,
    /// Cor de fundo do ícone em formato Hexadecimal (ex: "#00000000" para transparente)
    pub background_color: String,
    /// Ajuste fino horizontal em pixels
    pub offset_x: i32,
    /// Ajuste fino vertical em pixels
    pub offset_y: i32,
    /// Intervalo do timer de checagem (em milissegundos)
    pub poll_interval_ms: u64,
}

/// Implementação da trait `Default`.
/// Em Rust, em vez de construtores sobrecarregados como no C#, implementamos
/// `Default` para fornecer uma configuração inicial segura.
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            font_file: "seguisb.ttf".to_string(),
            font_size: 54.0,
            font_color: "#FFFFFF".to_string(),
            background_color: "#00000000".to_string(),
            offset_x: 0,
            offset_y: 0,
            poll_interval_ms: 200,
        }
    }
}

impl AppConfig {
    /// Retorna o caminho do arquivo `config.toml`.
    /// Prioriza sempre o `config.toml` que está na mesma pasta do `.exe`,
    /// evitando que o programa leia um arquivo diferente dependendo de onde foi iniciado!
    pub fn config_path() -> PathBuf {
        if let Ok(mut exe_path) = std::env::current_exe() {
            exe_path.pop(); // Remove o nome do .exe, ficando só com a pasta
            let beside_exe = exe_path.join("config.toml");
            if beside_exe.exists() {
                return beside_exe;
            }

            // Se não existir ao lado do .exe, mas existir no diretório atual (ex: durante `cargo run`)
            let local_path = PathBuf::from("config.toml");
            if local_path.exists() {
                return local_path;
            }

            return beside_exe;
        }

        PathBuf::from("config.toml")
    }

    /// Carrega a configuração do disco. Se o arquivo não existir, cria um com os
    /// valores padrão para facilitar a vida do usuário.
    pub fn load() -> Self {
        let path = Self::config_path();

        if !path.exists() {
            let default_cfg = Self::default();
            // Tenta salvar um config.toml padrão caso não exista
            if let Ok(toml_str) = toml::to_string_pretty(&default_cfg) {
                let _ = fs::write(&path, toml_str);
            }
            return default_cfg;
        }

        // Lê o arquivo como String e faz o parse do TOML
        match fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_else(|err| {
                eprintln!("Erro ao ler config.toml ({}), usando valores padrão.", err);
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Resolve o caminho completo do arquivo de fonte.
    /// Se o usuário colocou apenas "arialbd.ttf", buscamos em `C:\Windows\Fonts\arialbd.ttf`.
    pub fn resolve_font_path(&self) -> PathBuf {
        let candidate = Path::new(&self.font_file);
        if candidate.is_absolute() && candidate.exists() {
            return candidate.to_path_buf();
        }

        let windows_fonts = Path::new("C:\\Windows\\Fonts").join(&self.font_file);
        if windows_fonts.exists() {
            return windows_fonts;
        }

        // Fallback: se o usuário digitou um nome de arquivo que não existe,
        // garantimos que pelo menos a Segoe UI ou Arial padrão do Windows seja carregada.
        for fallback in ["seguisb.ttf", "segoeui.ttf", "arialbd.ttf", "arial.ttf"] {
            let fb_path = Path::new("C:\\Windows\\Fonts").join(fallback);
            if fb_path.exists() {
                return fb_path;
            }
        }

        windows_fonts
    }

    /// Converte uma string hexadecimal (ex: "#FFFFFF" ou "#FF5500CC") para um array RGBA `[u8; 4]`.
    pub fn parse_hex_color(hex: &str, default: [u8; 4]) -> [u8; 4] {
        let clean = hex.trim().trim_start_matches('#');
        match clean.len() {
            // Formato #RRGGBB (assume Alpha = 255 / totalmente opaco)
            6 => {
                let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(default[0]);
                let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(default[1]);
                let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(default[2]);
                [r, g, b, 255]
            }
            // Formato #RRGGBBAA (com canal de transparência explícito)
            8 => {
                let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(default[0]);
                let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(default[1]);
                let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(default[2]);
                let a = u8::from_str_radix(&clean[6..8], 16).unwrap_or(default[3]);
                [r, g, b, a]
            }
            _ => default,
        }
    }
}
