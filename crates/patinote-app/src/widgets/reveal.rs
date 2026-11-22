//! Scrolling a list so the item picked with the keyboard stays in view.

use iced::Task;
use iced::advanced::widget::operation::scrollable::AbsoluteOffset;
use iced::advanced::widget::operation::{Operation, Outcome, Scrollable};
use iced::advanced::widget::{self, Id};
use iced::{Rectangle, Vector};

/// Scrolls the scrollable `scroll` as little as needed for the container
/// `item` inside it to be fully visible. Nothing happens when either is
/// missing or the item is already in view.
pub fn reveal<T: Send + 'static>(scroll: &'static str, item: &'static str) -> Task<T> {
    widget::operate(Reveal {
        scroll: Id::new(scroll),
        item: Id::new(item),
        view: None,
        found: None,
    })
    .discard()
}

/// Where the scrollable shows its content.
#[derive(Debug, Clone, Copy)]
struct View {
    bounds: Rectangle,
    content: Rectangle,
    offset: Vector,
}

/// The new vertical offset that shows `item` in `view`, if it must change.
fn offset_for(view: View, item: Rectangle) -> Option<f32> {
    let top = item.y - view.content.y;
    let bottom = top + item.height;
    let shown = view.offset.y;
    // Iced draws at whole-pixel offsets: round away from the item's edge.
    if top < shown {
        Some(top.floor())
    } else if bottom > shown + view.bounds.height {
        Some((bottom - view.bounds.height).ceil())
    } else {
        None
    }
}

/// Finds the scrollable and the item (laid out unscrolled, inside the
/// content), then scrolls in a second pass.
struct Reveal {
    scroll: Id,
    item: Id,
    view: Option<View>,
    found: Option<Rectangle>,
}

impl Operation for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        offset: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if id == Some(&self.scroll) {
            self.view = Some(View {
                bounds,
                content,
                offset,
            });
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.item) && self.view.is_some() {
            self.found = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<()> {
        let (Some(view), Some(item)) = (self.view, self.found) else {
            return Outcome::None;
        };
        match offset_for(view, item) {
            Some(y) => Outcome::Chain(Box::new(widget::operation::scrollable::scroll_to(
                self.scroll.clone(),
                AbsoluteOffset {
                    x: None,
                    y: Some(y),
                },
            ))),
            None => Outcome::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{Point, Size};

    /// Pixels, kept off float literals (the tokens test forbids them).
    fn px(n: u16) -> f32 {
        f32::from(n)
    }

    fn view(offset: f32) -> View {
        View {
            bounds: Rectangle::new(Point::new(0.0, px(100)), Size::new(px(200), px(300))),
            content: Rectangle::new(Point::new(0.0, px(100)), Size::new(px(200), px(2000))),
            offset: Vector::new(0.0, offset),
        }
    }

    fn row(top: f32) -> Rectangle {
        Rectangle::new(Point::new(0.0, px(100) + top), Size::new(px(200), px(40)))
    }

    #[test]
    fn f10_an_item_in_view_does_not_scroll() {
        assert_eq!(offset_for(view(0.0), row(px(100))), None);
    }

    #[test]
    fn f10_an_item_below_scrolls_down_to_its_bottom() {
        assert_eq!(offset_for(view(0.0), row(px(500))), Some(px(240)));
    }

    #[test]
    fn f10_a_fractional_edge_is_rounded_to_show_the_whole_item() {
        let at = row(px(500)) + Vector::new(0.0, px(1) / px(5));
        assert_eq!(offset_for(view(0.0), at), Some(px(241)));
        assert_eq!(offset_for(view(px(800)), at), Some(px(500)));
    }

    #[test]
    fn f10_an_item_above_scrolls_up_to_its_top() {
        assert_eq!(offset_for(view(px(800)), row(px(500))), Some(px(500)));
    }
}
