// TODO: move somewhere else
pub enum MouseEvent {
    LeftMouseButtonDown,
    MiddleMouseButtonDown,
    RightMouseButtonDown,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    BackMouseButtonDown,
    ForwardMouseButtonDown,
}

pub trait GuiState: std::marker::Send {
    fn draw(&mut self);
    fn handle_mouse(&mut self, mouse_event: MouseEvent, x: i32, y: i32);
}