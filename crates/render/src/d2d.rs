use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Gdi::HDC;

use ystrokey_core::{
    DisplayItem, DisplayItemKind, KeyAction, KeyStrokeEntry, RenderError, StyleConfig,
};

pub struct D2DRenderer {
    #[allow(dead_code)]
    factory: ID2D1Factory1,
    render_target: ID2D1DCRenderTarget,
    #[allow(dead_code)]
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
    label_text_format: IDWriteTextFormat,
    count_text_format: IDWriteTextFormat,
    text_brush: ID2D1SolidColorBrush,
    // Up状態の文字色(濃い青)
    up_text_brush: ID2D1SolidColorBrush,
    // キー種別ごとの背景ブラシ
    key_down_brush: ID2D1SolidColorBrush,
    key_up_brush: ID2D1SolidColorBrush,
    modifier_brush: ID2D1SolidColorBrush,
    shortcut_brush: ID2D1SolidColorBrush,
    ime_brush: ID2D1SolidColorBrush,
    clipboard_brush: ID2D1SolidColorBrush,
    numpad_brush: ID2D1SolidColorBrush,
    lock_brush: ID2D1SolidColorBrush,
    count_brush: ID2D1SolidColorBrush,
    // Ghost-mode 用ブラシ
    ghost_bg_brush: ID2D1SolidColorBrush,
    ghost_border_brush: ID2D1SolidColorBrush,
    dpi_scale: f32,
}

impl D2DRenderer {
    pub fn new(style: &StyleConfig) -> Result<Self, RenderError> {
        unsafe {
            let factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
                    .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let render_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 0.0,
                dpiY: 0.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };

            let render_target = factory
                .CreateDCRenderTarget(&render_props)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let dwrite_factory: IDWriteFactory =
                DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
                    .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let font_wide = to_wide(&style.font_family);

            // メインテキストフォーマット
            let text_format = dwrite_factory
                .CreateTextFormat(
                    PCWSTR(font_wide.as_ptr()),
                    None,
                    DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    style.font_size,
                    w!("ja-JP"),
                )
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            // ショートカットラベル用フォーマット (85%)
            let label_text_format = dwrite_factory
                .CreateTextFormat(
                    PCWSTR(font_wide.as_ptr()),
                    None,
                    DWRITE_FONT_WEIGHT_MEDIUM,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    style.font_size * 0.85,
                    w!("ja-JP"),
                )
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            // 連打カウント用フォーマット (75%)
            let count_text_format = dwrite_factory
                .CreateTextFormat(
                    PCWSTR(font_wide.as_ptr()),
                    None,
                    DWRITE_FONT_WEIGHT_BOLD,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    style.font_size * 0.75,
                    w!("ja-JP"),
                )
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            // テキストブラシ (StyleConfig.text_color)
            let text_brush = render_target
                .CreateSolidColorBrush(&parse_color(&style.text_color), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            // Up状態テキストブラシ (#1565C0)
            let up_text_brush = render_target
                .CreateSolidColorBrush(&parse_color("#1565C0"), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            // 背景ブラシ群
            let key_down_brush = render_target
                .CreateSolidColorBrush(&parse_color(&style.key_down_color), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let key_up_brush = render_target
                .CreateSolidColorBrush(&parse_color("#90CAF9"), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let modifier_brush = render_target
                .CreateSolidColorBrush(&parse_color("#7C4DFF"), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let shortcut_brush = render_target
                .CreateSolidColorBrush(&parse_color(&style.shortcut_color), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let ime_brush = render_target
                .CreateSolidColorBrush(&parse_color("#F44336"), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let clipboard_brush = render_target
                .CreateSolidColorBrush(&parse_color("#FF9800"), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let numpad_brush = render_target
                .CreateSolidColorBrush(&parse_color("#009688"), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let lock_brush = render_target
                .CreateSolidColorBrush(&parse_color("#607D8B"), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let count_brush = render_target
                .CreateSolidColorBrush(&parse_color("#FF9800"), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            // Ghost-mode ブラシ: 暗めグレー背景 + 白枠線
            let ghost_bg_brush = render_target
                .CreateSolidColorBrush(&parse_color("#1A1A1A"), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            let ghost_border_brush = render_target
                .CreateSolidColorBrush(&parse_color("#FFFFFF"), None)
                .map_err(|e: windows::core::Error| RenderError::CreateFailed(e.to_string()))?;

            Ok(Self {
                factory,
                render_target,
                dwrite_factory,
                text_format,
                label_text_format,
                count_text_format,
                text_brush,
                up_text_brush,
                key_down_brush,
                key_up_brush,
                modifier_brush,
                shortcut_brush,
                ime_brush,
                clipboard_brush,
                numpad_brush,
                lock_brush,
                count_brush,
                ghost_bg_brush,
                ghost_border_brush,
                dpi_scale: 1.0,
            })
        }
    }

    /// DisplayItemKindに応じて背景ブラシを選択
    fn select_bg_brush(&self, item: &DisplayItem) -> &ID2D1SolidColorBrush {
        match &item.kind {
            DisplayItemKind::KeyStroke {
                label,
                modifiers,
                action,
                ..
            } => {
                if label.starts_with("Num") {
                    &self.numpad_brush
                } else if modifiers.any() {
                    &self.modifier_brush
                } else {
                    match action {
                        KeyAction::Down => &self.key_down_brush,
                        KeyAction::Up => &self.key_up_brush,
                    }
                }
            }
            DisplayItemKind::KeyStrokeGroup { strokes } => {
                // 先頭エントリの属性で代表色を返す
                if let Some(first) = strokes.first() {
                    if first.label.starts_with("Num") {
                        &self.numpad_brush
                    } else if first.modifiers.any() {
                        &self.modifier_brush
                    } else {
                        match first.action {
                            KeyAction::Down => &self.key_down_brush,
                            KeyAction::Up => &self.key_up_brush,
                        }
                    }
                } else {
                    &self.key_down_brush
                }
            }
            DisplayItemKind::Shortcut { .. } => &self.shortcut_brush,
            DisplayItemKind::ImeComposition { .. } => &self.ime_brush,
            DisplayItemKind::ClipboardPreview { .. } => &self.clipboard_brush,
            DisplayItemKind::LockIndicator { .. } => &self.lock_brush,
        }
    }

    /// KeyStrokeのUp状態かを判定（Up状態は文字色が異なる）
    fn is_up_state(item: &DisplayItem) -> bool {
        matches!(
            &item.kind,
            DisplayItemKind::KeyStroke {
                action: KeyAction::Up,
                ..
            }
        )
    }

    /// テキストブラシを選択（Up状態は濃い色）
    fn select_text_brush(&self, item: &DisplayItem) -> &ID2D1SolidColorBrush {
        if Self::is_up_state(item) {
            &self.up_text_brush
        } else {
            &self.text_brush
        }
    }

    /// StyleConfig変更時にブラシを再生成
    pub fn update_style(&mut self, style: &StyleConfig) {
        unsafe {
            if let Ok(b) = self.render_target.CreateSolidColorBrush(&parse_color(&style.text_color), None) {
                self.text_brush = b;
            }
            if let Ok(b) = self.render_target.CreateSolidColorBrush(&parse_color(&style.key_down_color), None) {
                self.key_down_brush = b;
            }
            if let Ok(b) = self.render_target.CreateSolidColorBrush(&parse_color(&style.shortcut_color), None) {
                self.shortcut_brush = b;
            }
        }
    }

    pub fn update_dpi(&mut self, dpi: u32) {
        self.dpi_scale = dpi as f32 / 96.0;
    }

    pub fn dpi_scale(&self) -> f32 {
        self.dpi_scale
    }

    pub fn render(
        &self,
        items: &[DisplayItem],
        style: &StyleConfig,
        hdc: HDC,
        width: u32,
        height: u32,
        ghost_opacity: f32,
    ) -> Result<(), RenderError> {
        unsafe {
            // DCをバインド
            let bind_rect = RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };
            self.render_target
                .BindDC(hdc, &bind_rect)
                .map_err(|e: windows::core::Error| RenderError::DrawFailed(e.to_string()))?;

            self.render_target.BeginDraw();

            // 透明クリア
            self.render_target.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            // Ghost背景（アイテム描画の前）
            self.render_ghost_background(items, style, ghost_opacity);

            let s = self.dpi_scale;
            let line_height = (style.font_size + style.padding * 2.0) * s;
            let spacing = 4.0_f32 * s;
            let size = self.render_target.GetSize();

            for (i, item) in items.iter().enumerate() {
                let bottom = size.height - (i as f32) * (line_height + spacing);
                let top = bottom - line_height;

                let bg_brush = self.select_bg_brush(item);
                let text_brush = self.select_text_brush(item);

                bg_brush.SetOpacity(item.opacity);
                text_brush.SetOpacity(item.opacity);

                match &item.kind {
                    DisplayItemKind::Shortcut {
                        keys_label,
                        action_label,
                    } => {
                        self.render_shortcut(
                            keys_label,
                            action_label,
                            top,
                            bottom,
                            size.width,
                            style,
                            item.opacity,
                        );
                    }
                    DisplayItemKind::KeyStrokeGroup { strokes } => {
                        self.render_keystroke_group(
                            strokes,
                            top,
                            bottom,
                            size.width,
                            style,
                            item.opacity,
                        );
                    }
                    DisplayItemKind::KeyStroke {
                        repeat_count,
                        ..
                    } if *repeat_count > 1 => {
                        let main_text = format_item_text_no_count(&item.kind);
                        let count_text = format!(" x{}", repeat_count);
                        self.render_keystroke_with_count(
                            &main_text,
                            &count_text,
                            top,
                            bottom,
                            size.width,
                            style,
                            bg_brush,
                            text_brush,
                            item.opacity,
                        );
                    }
                    _ => {
                        let text = format_item_text(&item.kind);
                        self.render_simple_item(
                            &text,
                            top,
                            bottom,
                            size.width,
                            style,
                            bg_brush,
                            text_brush,
                        );
                    }
                }
            }

            self.render_target
                .EndDraw(None, None)
                .map_err(|e: windows::core::Error| RenderError::DrawFailed(e.to_string()))?;
        }

        Ok(())
    }

    /// Ghost-mode: アクティブアイテムの背後に半透明背景を描画
    unsafe fn render_ghost_background(
        &self,
        items: &[DisplayItem],
        style: &StyleConfig,
        ghost_opacity: f32,
    ) {
        if ghost_opacity <= 0.0 {
            return;
        }

        let s = self.dpi_scale;
        let line_height = (style.font_size + style.padding * 2.0) * s;
        let spacing = 4.0_f32 * s;
        let size = self.render_target.GetSize();

        // アイテムがなくても最低1行分のサイズを確保（ドラッグハンドル用）
        let visible_lines = items.len().max(1) as f32;
        let total_height = visible_lines * (line_height + spacing) - spacing;
        let bottom = size.height;
        let top = bottom - total_height;

        let margin = 4.0_f32;
        let bg_rect = D2D_RECT_F {
            left: margin,
            top: top - margin,
            right: size.width - margin,
            bottom: bottom + margin,
        };

        let rounded = D2D1_ROUNDED_RECT {
            rect: bg_rect,
            radiusX: style.border_radius + 4.0,
            radiusY: style.border_radius + 4.0,
        };

        // 暗め背景塗り
        self.ghost_bg_brush.SetOpacity(ghost_opacity * 0.3);
        self.render_target
            .FillRoundedRectangle(&rounded, &self.ghost_bg_brush);

        // 白枠線
        self.ghost_border_brush.SetOpacity(ghost_opacity * 0.15);
        self.render_target
            .DrawRoundedRectangle(&rounded, &self.ghost_border_brush, 1.0, None);
    }

    /// 通常アイテムの描画
    #[allow(clippy::too_many_arguments)]
    unsafe fn render_simple_item(
        &self,
        text: &str,
        top: f32,
        bottom: f32,
        width: f32,
        style: &StyleConfig,
        bg_brush: &ID2D1SolidColorBrush,
        text_brush: &ID2D1SolidColorBrush,
    ) {
        let rect = D2D_RECT_F {
            left: style.padding,
            top,
            right: width - style.padding,
            bottom,
        };

        let rounded = D2D1_ROUNDED_RECT {
            rect,
            radiusX: style.border_radius,
            radiusY: style.border_radius,
        };

        self.render_target
            .FillRoundedRectangle(&rounded, bg_brush);

        let text_rect = D2D_RECT_F {
            left: rect.left + style.padding,
            top: rect.top + style.padding / 2.0,
            right: rect.right - style.padding,
            bottom: rect.bottom - style.padding / 2.0,
        };

        let text_wide: Vec<u16> = text.encode_utf16().collect();
        self.render_target.DrawText(
            &text_wide,
            &self.text_format,
            &text_rect,
            text_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    /// ショートカット: keys_label(左) + action_label(右、緑バッジ)
    #[allow(clippy::too_many_arguments)]
    unsafe fn render_shortcut(
        &self,
        keys_label: &str,
        action_label: &str,
        top: f32,
        bottom: f32,
        width: f32,
        style: &StyleConfig,
        opacity: f32,
    ) {
        // 背景全体（紫ベース: 修飾キー込みのショートカット）
        let rect = D2D_RECT_F {
            left: style.padding,
            top,
            right: width - style.padding,
            bottom,
        };

        let rounded = D2D1_ROUNDED_RECT {
            rect,
            radiusX: style.border_radius,
            radiusY: style.border_radius,
        };

        self.modifier_brush.SetOpacity(opacity);
        self.render_target
            .FillRoundedRectangle(&rounded, &self.modifier_brush);

        // keys_label（左側、白文字）
        let keys_rect = D2D_RECT_F {
            left: rect.left + style.padding,
            top: rect.top + style.padding / 2.0,
            right: rect.right - style.padding,
            bottom: rect.bottom - style.padding / 2.0,
        };

        self.text_brush.SetOpacity(opacity);
        let keys_wide: Vec<u16> = keys_label.encode_utf16().collect();
        self.render_target.DrawText(
            &keys_wide,
            &self.text_format,
            &keys_rect,
            &self.text_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        // action_label（右側、緑バッジ）
        // keys_labelの幅を計測してバッジ位置を決定
        let keys_text_layout = self
            .dwrite_factory
            .CreateTextLayout(
                &keys_wide,
                &self.text_format,
                rect.right - rect.left,
                bottom - top,
            );

        if let Ok(layout) = keys_text_layout {
            let mut metrics = DWRITE_TEXT_METRICS::default();
            let _ = layout.GetMetrics(&mut metrics);
            let keys_width = metrics.width;

            let badge_left = rect.left + style.padding + keys_width + 8.0;
            let badge_padding = 6.0_f32;

            // action_labelの幅を計測
            let action_wide: Vec<u16> = action_label.encode_utf16().collect();
            let action_layout = self
                .dwrite_factory
                .CreateTextLayout(
                    &action_wide,
                    &self.label_text_format,
                    rect.right - badge_left,
                    bottom - top,
                );

            if let Ok(a_layout) = action_layout {
                let mut a_metrics = DWRITE_TEXT_METRICS::default();
                let _ = a_layout.GetMetrics(&mut a_metrics);
                let action_width = a_metrics.width;

                let badge_rect = D2D_RECT_F {
                    left: badge_left,
                    top: top + 3.0,
                    right: badge_left + action_width + badge_padding * 2.0,
                    bottom: bottom - 3.0,
                };

                let badge_rounded = D2D1_ROUNDED_RECT {
                    rect: badge_rect,
                    radiusX: 4.0,
                    radiusY: 4.0,
                };

                self.shortcut_brush.SetOpacity(opacity);
                self.render_target
                    .FillRoundedRectangle(&badge_rounded, &self.shortcut_brush);

                let action_text_rect = D2D_RECT_F {
                    left: badge_rect.left + badge_padding,
                    top: badge_rect.top,
                    right: badge_rect.right - badge_padding,
                    bottom: badge_rect.bottom,
                };

                self.render_target.DrawText(
                    &action_wide,
                    &self.label_text_format,
                    &action_text_rect,
                    &self.text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    /// 連打カウント付きキーストローク描画
    #[allow(clippy::too_many_arguments)]
    unsafe fn render_keystroke_with_count(
        &self,
        main_text: &str,
        count_text: &str,
        top: f32,
        bottom: f32,
        width: f32,
        style: &StyleConfig,
        bg_brush: &ID2D1SolidColorBrush,
        text_brush: &ID2D1SolidColorBrush,
        opacity: f32,
    ) {
        // 背景
        let rect = D2D_RECT_F {
            left: style.padding,
            top,
            right: width - style.padding,
            bottom,
        };

        let rounded = D2D1_ROUNDED_RECT {
            rect,
            radiusX: style.border_radius,
            radiusY: style.border_radius,
        };

        self.render_target
            .FillRoundedRectangle(&rounded, bg_brush);

        // メインテキスト
        let text_rect = D2D_RECT_F {
            left: rect.left + style.padding,
            top: rect.top + style.padding / 2.0,
            right: rect.right - style.padding,
            bottom: rect.bottom - style.padding / 2.0,
        };

        let main_wide: Vec<u16> = main_text.encode_utf16().collect();
        self.render_target.DrawText(
            &main_wide,
            &self.text_format,
            &text_rect,
            text_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        // メインテキスト幅を計測してカウント位置を決定
        let main_layout = self.dwrite_factory.CreateTextLayout(
            &main_wide,
            &self.text_format,
            rect.right - rect.left,
            bottom - top,
        );

        if let Ok(layout) = main_layout {
            let mut metrics = DWRITE_TEXT_METRICS::default();
            let _ = layout.GetMetrics(&mut metrics);
            let main_width = metrics.width;

            let count_left = rect.left + style.padding + main_width;

            let count_rect = D2D_RECT_F {
                left: count_left,
                top: rect.top + style.padding / 2.0,
                right: rect.right - style.padding,
                bottom: rect.bottom - style.padding / 2.0,
            };

            self.count_brush.SetOpacity(opacity);
            let count_wide: Vec<u16> = count_text.encode_utf16().collect();
            self.render_target.DrawText(
                &count_wide,
                &self.count_text_format,
                &count_rect,
                &self.count_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    /// 連続キーストロークグループの水平描画
    #[allow(clippy::too_many_arguments)]
    unsafe fn render_keystroke_group(
        &self,
        strokes: &[KeyStrokeEntry],
        top: f32,
        bottom: f32,
        width: f32,
        style: &StyleConfig,
        opacity: f32,
    ) {
        let pill_gap = 4.0_f32;
        let pill_padding_h = 8.0_f32;
        let pill_padding_v = 3.0_f32;
        let pill_radius = 4.0_f32;
        let mut cursor_x = style.padding;

        for entry in strokes {
            // テキスト生成
            let text = format_entry_text(entry);
            let text_wide: Vec<u16> = text.encode_utf16().collect();

            // テキスト幅計測
            let layout = self.dwrite_factory.CreateTextLayout(
                &text_wide,
                &self.text_format,
                width,
                bottom - top,
            );
            let text_width = if let Ok(layout) = layout {
                let mut metrics = DWRITE_TEXT_METRICS::default();
                let _ = layout.GetMetrics(&mut metrics);
                metrics.width
            } else {
                // フォールバック: 文字数ベース概算
                text.len() as f32 * style.font_size * 0.6
            };

            let pill_width = text_width + pill_padding_h * 2.0;

            // 画面幅超過時は打ち切り
            if cursor_x + pill_width > width - style.padding {
                break;
            }

            // ピル背景ブラシ選択
            let bg_brush = self.select_entry_bg_brush(entry);
            bg_brush.SetOpacity(opacity);

            // テキストブラシ選択
            let text_brush = if matches!(entry.action, KeyAction::Up) {
                &self.up_text_brush
            } else {
                &self.text_brush
            };
            text_brush.SetOpacity(opacity);

            // ピル背景描画
            let pill_rect = D2D_RECT_F {
                left: cursor_x,
                top: top + pill_padding_v,
                right: cursor_x + pill_width,
                bottom: bottom - pill_padding_v,
            };
            let pill_rounded = D2D1_ROUNDED_RECT {
                rect: pill_rect,
                radiusX: pill_radius,
                radiusY: pill_radius,
            };
            self.render_target
                .FillRoundedRectangle(&pill_rounded, bg_brush);

            // テキスト描画
            let text_rect = D2D_RECT_F {
                left: pill_rect.left + pill_padding_h,
                top: pill_rect.top,
                right: pill_rect.right - pill_padding_h,
                bottom: pill_rect.bottom,
            };
            self.render_target.DrawText(
                &text_wide,
                &self.text_format,
                &text_rect,
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 連打カウント表示
            if entry.repeat_count > 1 {
                let count_text = format!("x{}", entry.repeat_count);
                let count_wide: Vec<u16> = count_text.encode_utf16().collect();
                let count_layout = self.dwrite_factory.CreateTextLayout(
                    &count_wide,
                    &self.count_text_format,
                    width,
                    bottom - top,
                );
                if let Ok(cl) = count_layout {
                    let mut cm = DWRITE_TEXT_METRICS::default();
                    let _ = cl.GetMetrics(&mut cm);
                    let count_left = pill_rect.right - pill_padding_h / 2.0;
                    let count_rect = D2D_RECT_F {
                        left: count_left,
                        top: pill_rect.top - 2.0,
                        right: count_left + cm.width + 4.0,
                        bottom: pill_rect.top + cm.height,
                    };
                    self.count_brush.SetOpacity(opacity);
                    self.render_target.DrawText(
                        &count_wide,
                        &self.count_text_format,
                        &count_rect,
                        &self.count_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            }

            cursor_x += pill_width + pill_gap;
        }
    }

    /// KeyStrokeEntry から背景ブラシを選択
    fn select_entry_bg_brush(&self, entry: &KeyStrokeEntry) -> &ID2D1SolidColorBrush {
        if entry.label.starts_with("Num") {
            &self.numpad_brush
        } else if entry.modifiers.any() {
            &self.modifier_brush
        } else {
            match entry.action {
                KeyAction::Down => &self.key_down_brush,
                KeyAction::Up => &self.key_up_brush,
            }
        }
    }
}

/// 連打カウントなしのテキスト生成
fn format_item_text_no_count(kind: &DisplayItemKind) -> String {
    match kind {
        DisplayItemKind::KeyStroke {
            label,
            modifiers,
            ..
        } => {
            let mut s = String::new();
            if modifiers.ctrl {
                s.push_str("Ctrl+");
            }
            if modifiers.alt {
                s.push_str("Alt+");
            }
            if modifiers.shift {
                s.push_str("Shift+");
            }
            if modifiers.win {
                s.push_str("Win+");
            }
            s.push_str(label);
            s
        }
        other => format_item_text(other),
    }
}

fn format_item_text(kind: &DisplayItemKind) -> String {
    match kind {
        DisplayItemKind::KeyStroke {
            label,
            modifiers,
            repeat_count,
            ..
        } => {
            let mut s = String::new();
            if modifiers.ctrl {
                s.push_str("Ctrl+");
            }
            if modifiers.alt {
                s.push_str("Alt+");
            }
            if modifiers.shift {
                s.push_str("Shift+");
            }
            if modifiers.win {
                s.push_str("Win+");
            }
            s.push_str(label);
            if *repeat_count > 1 {
                s.push_str(&format!(" x{}", repeat_count));
            }
            s
        }
        DisplayItemKind::KeyStrokeGroup { strokes } => {
            strokes
                .iter()
                .map(format_entry_text)
                .collect::<Vec<_>>()
                .join(" ")
        }
        DisplayItemKind::Shortcut {
            keys_label,
            action_label,
        } => {
            format!("{} ({})", keys_label, action_label)
        }
        DisplayItemKind::ImeComposition { text } => text.clone(),
        DisplayItemKind::ClipboardPreview { text } => {
            format!("[Clipboard] {}", text)
        }
        DisplayItemKind::LockIndicator { caps, num, scroll } => {
            let mut parts = Vec::new();
            if *caps {
                parts.push("CAPS");
            }
            if *num {
                parts.push("NUM");
            }
            if *scroll {
                parts.push("SCROLL");
            }
            parts.join(" | ")
        }
    }
}

/// KeyStrokeEntry のテキスト生成（修飾キー付き）
fn format_entry_text(entry: &KeyStrokeEntry) -> String {
    let mut s = String::new();
    if entry.modifiers.ctrl {
        s.push_str("Ctrl+");
    }
    if entry.modifiers.alt {
        s.push_str("Alt+");
    }
    if entry.modifiers.shift {
        s.push_str("Shift+");
    }
    if entry.modifiers.win {
        s.push_str("Win+");
    }
    s.push_str(&entry.label);
    if entry.repeat_count > 1 {
        s.push_str(&format!(" x{}", entry.repeat_count));
    }
    s
}

/// "#RRGGBB" or "#RRGGBBAA" 形式をD2D1_COLOR_Fに変換
pub fn parse_color(hex: &str) -> D2D1_COLOR_F {
    let hex = hex.trim_start_matches('#');
    let (r, g, b, a) = match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            (r, g, b, 255u8)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            let a = u8::from_str_radix(&hex[6..8], 16).unwrap_or(255);
            (r, g, b, a)
        }
        _ => (0, 0, 0, 255),
    };
    D2D1_COLOR_F {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    }
}

/// &strをnull終端UTF-16に変換
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
