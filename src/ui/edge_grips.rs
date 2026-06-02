//! A transparent wrapper widget that reports `mouse::Interaction::Resizing*`
//! when the cursor is near the edges of its bounds. Pair it with the
//! `iced::window::drag_resize` task fired from `InputMouseDown` to give the
//! user proper resize affordance on a custom-decorated window (where the OS
//! no longer paints its own resize handles).
//!
//! The widget is otherwise invisible: layout, draw, events, and child
//! cursor interactions are all forwarded to its inner content.

use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::{mouse, Element, Event, Length, Rectangle, Size, Vector};

pub struct EdgeGrips<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    enabled: bool,
    edge_grip: f32,
    corner_grip: f32,
}

impl<'a, Message, Theme, Renderer> EdgeGrips<'a, Message, Theme, Renderer> {
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            content: content.into(),
            enabled: true,
            edge_grip: 10.0,
            corner_grip: 16.0,
        }
    }

    /// Skip the resize-cursor logic. Use in fullscreen or when OS decorations
    /// already provide their own edge handles.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for EdgeGrips<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.enabled {
            if let Some(interaction) = self.edge_interaction(cursor, layout.bounds()) {
                return interaction;
            }
        }
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> EdgeGrips<'a, Message, Theme, Renderer> {
    /// Returns the resize cursor for an edge / corner zone, or None when
    /// the cursor is in the interior.
    fn edge_interaction(
        &self,
        cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Option<mouse::Interaction> {
        let p = cursor.position()?;
        let x = p.x - bounds.x;
        let y = p.y - bounds.y;
        let w = bounds.width;
        let h = bounds.height;
        if x < 0.0 || y < 0.0 || x > w || y > h {
            return None;
        }

        let near_l = x <= self.corner_grip;
        let near_r = x >= w - self.corner_grip;
        let near_t = y <= self.corner_grip;
        let near_b = y >= h - self.corner_grip;

        // Corners first - when two adjacent edges are both in range.
        if (near_l && near_t) || (near_r && near_b) {
            return Some(mouse::Interaction::ResizingDiagonallyDown); // ↖↘ (\)
        }
        if (near_r && near_t) || (near_l && near_b) {
            return Some(mouse::Interaction::ResizingDiagonallyUp); // ↗↙ (/)
        }

        // Straight edges, using the tighter edge_grip.
        if x <= self.edge_grip || x >= w - self.edge_grip {
            return Some(mouse::Interaction::ResizingHorizontally);
        }
        if y <= self.edge_grip || y >= h - self.edge_grip {
            return Some(mouse::Interaction::ResizingVertically);
        }
        None
    }
}

impl<'a, Message, Theme, Renderer> From<EdgeGrips<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(value: EdgeGrips<'a, Message, Theme, Renderer>) -> Self {
        Element::new(value)
    }
}
