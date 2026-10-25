// ============================================================================
// MÓDULO: icon_renderer.rs
// OBJETIVO: Desenhar o número do Desktop Virtual em memória (buffer RGBA 64x64)
//           e convertê-lo em um `tray_icon::Icon` sem usar GDI legado.
//
// POR QUE ISSO É MELHOR QUE A VERSÃO C# (`Util/Icons.cs`)?
// - No C#, o método `bitmap.GetHicon()` alocava um handle GDI nativo no kernel
//   do Windows (`HICON`) que facilmente vazava memória (Bug H1 do relatório).
// - Aqui em Rust, desenhamos os pixels diretamente num `Vec<u8>` na memória RAM
//   usando a crate `ab_glyph` (rasterizador de fontes puro em Rust) e entregamos
//   para `tray_icon::Icon::from_rgba`. Quando o ícone antigo é substituído,
//   o Rust chama `Drop` automaticamente e destrói o recurso no Windows!
// ============================================================================

use crate::config::AppConfig;
use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use std::collections::HashMap;
use std::fs;
use tray_icon::Icon;

const ICON_SIZE: u32 = 64;

pub struct IconRenderer {
    font: FontVec,
    config: AppConfig,
    /// Cache simples de ícones já gerados (ex: Desktop 1, Desktop 2) para
    /// gastar 0% de CPU quando você alterna entre áreas de trabalho que já visitou.
    cache: HashMap<String, Icon>,
}

impl IconRenderer {
    /// Cria um novo renderizador carregando o arquivo `.ttf` do Windows na memória.
    pub fn new(config: AppConfig) -> Result<Self, String> {
        let font_path = config.resolve_font_path();
        let font_bytes = fs::read(&font_path).map_err(|e| {
            format!(
                "Não foi possível ler o arquivo de fonte em {:?}: {}",
                font_path, e
            )
        })?;

        let font = FontVec::try_from_vec(font_bytes)
            .map_err(|_| format!("Arquivo de fonte inválido: {:?}", font_path))?;

        Ok(Self {
            font,
            config,
            cache: HashMap::new(),
        })
    }

    /// Atualiza a configuração (ex: quando o usuário clica em "Recarregar Configuração")
    /// e limpa o cache de ícones para forçar o redesenho com a nova cor/fonte.
    pub fn reload_config(&mut self, new_config: AppConfig) -> Result<(), String> {
        let font_path = new_config.resolve_font_path();
        let font_bytes = fs::read(&font_path)
            .map_err(|e| format!("Erro ao recarregar fonte {:?}: {}", font_path, e))?;
        let font = FontVec::try_from_vec(font_bytes)
            .map_err(|_| format!("Fonte inválida ao recarregar: {:?}", font_path))?;

        self.font = font;
        self.config = new_config;
        self.cache.clear();
        Ok(())
    }

    /// Gera (ou busca no cache) o ícone de 64x64 pixels contendo o texto (ex: "1", "2", "10").
    pub fn render_text_icon(&mut self, text: &str) -> Result<Icon, String> {
        if let Some(cached) = self.cache.get(text) {
            return Ok(cached.clone());
        }

        let fg = AppConfig::parse_hex_color(&self.config.font_color, [255, 255, 255, 255]);
        let bg = AppConfig::parse_hex_color(&self.config.background_color, [0, 0, 0, 0]);

        // Criamos um buffer RGBA de 64x64 pixels (cada pixel ocupa 4 bytes: R, G, B, A)
        let mut rgba = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];

        // Preenche o fundo com a cor configurada (`background_color`)
        for pixel in rgba.chunks_exact_mut(4) {
            pixel[0] = bg[0];
            pixel[1] = bg[1];
            pixel[2] = bg[2];
            pixel[3] = bg[3];
        }

        // Se o texto tiver 2 dígitos (ex: Desktop "10"), reduzimos levemente a escala
        // automaticamente para que caiba com folga dentro do quadrado 64x64.
        let mut effective_size = self.config.font_size;
        if text.chars().count() >= 2 {
            effective_size *= 0.78;
        }

        let scale = PxScale::from(effective_size);
        let scaled_font = self.font.as_scaled(scale);

        // 1º Passo: Medir a largura total e a caixa delimitadora (bounding box) real dos glifos
        // para podermos centralizar o número perfeitamente tanto na horizontal quanto na vertical.
        let mut glyphs = Vec::new();
        let mut cursor_x = 0.0f32;
        let mut prev_glyph_id = None;

        for ch in text.chars() {
            let glyph_id = scaled_font.glyph_id(ch);
            if let Some(prev) = prev_glyph_id {
                cursor_x += scaled_font.kern(prev, glyph_id);
            }
            let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(cursor_x, 0.0));
            cursor_x += scaled_font.h_advance(glyph_id);
            prev_glyph_id = Some(glyph_id);
            glyphs.push(glyph);
        }

        // Descobre os limites exatos (min_x, min_y, max_x, max_y) dos pixels desenháveis
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        let mut outlined_glyphs = Vec::new();
        for glyph in glyphs {
            if let Some(outlined) = scaled_font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                min_x = min_x.min(bounds.min.x);
                min_y = min_y.min(bounds.min.y);
                max_x = max_x.max(bounds.max.x);
                max_y = max_y.max(bounds.max.y);
                outlined_glyphs.push(outlined);
            }
        }

        // 2º Passo: Desenhar cada pixel com Anti-Aliasing (suavização de bordas)
        if !outlined_glyphs.is_empty() {
            let text_width = max_x - min_x;
            let text_height = max_y - min_y;

            // Calcula onde o desenho deve começar para ficar no centro do ícone 64x64
            let start_x = ((ICON_SIZE as f32 - text_width) / 2.0) - min_x + self.config.offset_x as f32;
            let start_y = ((ICON_SIZE as f32 - text_height) / 2.0) - min_y + self.config.offset_y as f32;

            for outlined in outlined_glyphs {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    // `coverage` varia de 0.0 (transparente) a 1.0 (totalmente preenchido pela fonte)
                    let px = (bounds.min.x + gx as f32 + start_x).round() as i32;
                    let py = (bounds.min.y + gy as f32 + start_y).round() as i32;

                    if px >= 0 && px < ICON_SIZE as i32 && py >= 0 && py < ICON_SIZE as i32 {
                        let idx = ((py as u32 * ICON_SIZE + px as u32) * 4) as usize;
                        let alpha = (coverage * (fg[3] as f32 / 255.0)).clamp(0.0, 1.0);

                        // Alpha blending suave sobre o fundo
                        let inv_alpha = 1.0 - alpha;
                        rgba[idx] = (fg[0] as f32 * alpha + rgba[idx] as f32 * inv_alpha) as u8;
                        rgba[idx + 1] = (fg[1] as f32 * alpha + rgba[idx + 1] as f32 * inv_alpha) as u8;
                        rgba[idx + 2] = (fg[2] as f32 * alpha + rgba[idx + 2] as f32 * inv_alpha) as u8;
                        rgba[idx + 3] = ((alpha * 255.0) + (rgba[idx + 3] as f32 * inv_alpha)).min(255.0) as u8;
                    }
                });
            }
        }

        let icon = Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE)
            .map_err(|e| format!("Falha ao criar ícone RGBA: {}", e))?;

        self.cache.insert(text.to_string(), icon.clone());
        Ok(icon)
    }
}
