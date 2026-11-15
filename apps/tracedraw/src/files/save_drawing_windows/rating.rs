//! The shell's five-star rating editor and its document-value conversion.

use windows::core::{Error, Interface, Result};
use windows::Win32::Foundation::{E_POINTER, PROPERTYKEY};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::UI::Shell::IFileSaveDialog;
use windows::Win32::UI::Shell::PropertiesSystem::{
    IPropertyDescriptionList, IPropertyStore, PSCreateMemoryPropertyStore,
    PSGetPropertyDescriptionListFromString, PSGetPropertyKeyFromName,
};

/// Collect the built-in rating property using the shell's star editor.
pub(super) fn configure(save: &IFileSaveDialog, rating: u8) -> Result<()> {
    let store = property_store(rating)?;
    let mut description: Option<IPropertyDescriptionList> = None;
    // SAFETY: the output is a nullable COM interface. The API initializes it,
    // and the resulting references remain in this thread's apartment.
    unsafe {
        PSGetPropertyDescriptionListFromString(
            windows::core::w!("prop:System.Rating"),
            &IPropertyDescriptionList::IID,
            (&mut description as *mut Option<IPropertyDescriptionList>).cast(),
        )?;
        let description = description.ok_or_else(|| Error::from_hresult(E_POINTER))?;
        save.SetCollectedProperties(&description, false)?;
        save.SetProperties(&store)
    }
}

/// Read the final choice from the system's retained property store.
pub(super) fn read(save: &IFileSaveDialog) -> Result<u8> {
    // SAFETY: property collection was enabled before showing this dialog.
    let store = unsafe { save.GetProperties()? };
    read_store(&store)
}

fn property_store(rating: u8) -> Result<IPropertyStore> {
    let mut store: Option<IPropertyStore> = None;
    // SAFETY: the output has the nullable COM interface representation. The
    // store copies the variant value, whose binding releases it automatically.
    unsafe {
        PSCreateMemoryPropertyStore(
            &IPropertyStore::IID,
            (&mut store as *mut Option<IPropertyStore>).cast(),
        )?;
        let store = store.ok_or_else(|| Error::from_hresult(E_POINTER))?;
        store.SetValue(&rating_key()?, &PROPVARIANT::from(to_shell_rating(rating)))?;
        Ok(store)
    }
}

fn read_store(store: &IPropertyStore) -> Result<u8> {
    // SAFETY: the property key is supplied by the registered system schema.
    // PROPVARIANT owns its value and clears it when it leaves this scope.
    let value = unsafe { store.GetValue(&rating_key()?)? };
    if value.is_empty() {
        return Ok(0);
    }
    Ok(from_shell_rating(u32::try_from(&value)?))
}

fn rating_key() -> Result<PROPERTYKEY> {
    let mut key = PROPERTYKEY::default();
    // SAFETY: the API fills a caller-owned key from a system property name.
    unsafe { PSGetPropertyKeyFromName(windows::core::w!("System.Rating"), &mut key)? };
    Ok(key)
}

fn to_shell_rating(rating: u8) -> u32 {
    match rating.min(5) {
        0 => 0,
        1 => 1,
        2 => 25,
        3 => 50,
        4 => 75,
        _ => 99,
    }
}

fn from_shell_rating(rating: u32) -> u8 {
    match rating {
        0 => 0,
        1..=12 => 1,
        13..=37 => 2,
        38..=62 => 3,
        63..=87 => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_rating_uses_the_system_star_values_and_boundaries() {
        for (stars, value) in [(0, 0), (1, 1), (2, 25), (3, 50), (4, 75), (5, 99)] {
            assert_eq!(to_shell_rating(stars), value);
            assert_eq!(from_shell_rating(value), stars);
        }
        for (value, stars) in [
            (12, 1),
            (13, 2),
            (37, 2),
            (38, 3),
            (62, 3),
            (63, 4),
            (87, 4),
            (88, 5),
        ] {
            assert_eq!(from_shell_rating(value), stars);
        }
        assert_eq!(to_shell_rating(u8::MAX), 99);
        assert_eq!(from_shell_rating(u32::MAX), 5);
    }

    #[test]
    fn native_rating_round_trips_through_the_memory_property_store() {
        let _apartment = super::super::Apartment::enter().expect("STA");
        for rating in 0..=5u8 {
            let store = property_store(rating).expect("rating store");
            assert_eq!(read_store(&store).expect("rating value"), rating);
        }
    }

    #[test]
    fn native_rating_empty_property_clears_the_rating() {
        let _apartment = super::super::Apartment::enter().expect("STA");
        let store = property_store(4).expect("rating store");
        // SAFETY: the live store copies the empty value for the system key.
        unsafe {
            store
                .SetValue(&rating_key().expect("rating key"), &PROPVARIANT::default())
                .expect("clear rating");
        }
        assert_eq!(read_store(&store).expect("unrated value"), 0);
    }
}
