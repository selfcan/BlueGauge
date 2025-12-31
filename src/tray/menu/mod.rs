pub mod about;
pub mod handler;
pub mod item;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MenuGroup {
    CheckBoxNotify,
    CheckBoxTrayTooltip,
    RadioDevice,
    RadioLowBattery,
    RadioTrayIconStyle,
}
