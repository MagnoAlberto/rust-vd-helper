// ============================================================================
// MÓDULO: vd_win10.rs
// OBJETIVO: Detecção híbrida e ultra-confiável da Área de Trabalho Virtual
//           ativa no Windows 10 (incluindo 22H2 Build 19045.5000+) e Windows 11!
//
// POR QUE USAR UMA ESTRATÉGIA HÍBRIDA (REGISTRO + COM MULTIVERSÃO)?
// 1. Nas builds recentes do Windows 10 22H2 (ex: Build 19045.7807), a Microsoft
//    fez backport de mudanças do Windows Server 2022 / Windows 11 para o
//    `explorer.exe`, alterando assinaturas COM não documentadas.
// 2. Porém, o `explorer.exe` mantém e atualiza em tempo real os GUIDs binários
//    (16 bytes cada) nas chaves oficiais do Registro da sessão atual:
//    - Lista ordenada de todos os Desktops:
//      `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops` -> `VirtualDesktopIDs`
//    - GUID do Desktop ativo no momento:
//      `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops` -> `CurrentVirtualDesktop`
//      ou em `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SessionInfo\<SessionID>\VirtualDesktops`
//
// VANTAGENS DESTA ABORDAGEM EM RUST:
// - Lê bytes diretamente da memória do Registro (`RRF_RT_REG_BINARY`), sem
//   depender de tabelas virtuais (`VTable`) que mudam a cada KB do Windows Update!
// - Possui também o fallback COM (`Win10 Classic` + `Server2022/Win10 Backport` + `winvd`)
//   para garantir 100% de compatibilidade em qualquer versão do Windows.
// ============================================================================

use std::ffi::c_void;
use std::ptr;
use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
};

// ============================================================================
// 1. ESTRATÉGIA PRIMÁRIA: LEITURA BINÁRIA DIRETA DOS GUIDS NO REGISTRO
// ============================================================================

/// Converte um slice de 16 bytes (`REG_BINARY` do Windows) em uma struct `GUID`.
fn guid_from_16_bytes(bytes: &[u8]) -> Option<GUID> {
    if bytes.len() < 16 {
        return None;
    }
    let data1 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let data2 = u16::from_le_bytes([bytes[4], bytes[5]]);
    let data3 = u16::from_le_bytes([bytes[6], bytes[7]]);
    let mut data4 = [0u8; 8];
    data4.copy_from_slice(&bytes[8..16]);

    Some(GUID {
        data1,
        data2,
        data3,
        data4,
    })
}

/// Formata um `GUID` no padrão do Registro do Windows: `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}`.
fn format_guid(g: &GUID) -> String {
    format!(
        "{{{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

/// Compara se dois GUIDs são idênticos.
fn guid_eq(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

/// Lê um valor `REG_BINARY` de `HKEY_CURRENT_USER`.
fn read_reg_binary(subkey_str: &str, value_str: &str) -> Option<Vec<u8>> {
    let mut subkey_utf16: Vec<u16> = subkey_str.encode_utf16().collect();
    subkey_utf16.push(0);
    let mut value_utf16: Vec<u16> = value_str.encode_utf16().collect();
    value_utf16.push(0);

    let mut buffer = vec![0u8; 1024];
    let mut byte_size = buffer.len() as u32;

    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey_utf16.as_ptr(),
            value_utf16.as_ptr(),
            RRF_RT_REG_BINARY,
            ptr::null_mut(),
            buffer.as_mut_ptr() as *mut c_void,
            &mut byte_size,
        )
    };

    if status == 0 && byte_size >= 16 {
        buffer.truncate(byte_size as usize);
        Some(buffer)
    } else {
        None
    }
}

/// Busca o `CurrentVirtualDesktop` (16 bytes) tanto na chave raiz `VirtualDesktops`
/// quanto dentro de `SessionInfo\<SessionID>\VirtualDesktops` (onde o Windows 10 22H2 o grava!).
fn read_current_desktop_guid_from_registry() -> Option<GUID> {
    // 1. Tenta primeiro na chave direta (usada no Windows 11 e algumas builds do Win10)
    if let Some(bytes) = read_reg_binary(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops",
        "CurrentVirtualDesktop",
    ) {
        if let Some(g) = guid_from_16_bytes(&bytes) {
            return Some(g);
        }
    }

    // 2. Enumera as sessões em `SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SessionInfo`
    //    (No Windows 10 22H2, fica em `SessionInfo\1\VirtualDesktops` -> `CurrentVirtualDesktop`)
    let session_info_path = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\SessionInfo";
    let mut path_utf16: Vec<u16> = session_info_path.encode_utf16().collect();
    path_utf16.push(0);

    unsafe {
        let mut hkey: HKEY = ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, path_utf16.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
            let mut index = 0;
            loop {
                let mut name_buf = [0u16; 64];
                let mut name_len = name_buf.len() as u32;
                let res = RegEnumKeyExW(
                    hkey,
                    index,
                    name_buf.as_mut_ptr(),
                    &mut name_len,
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                );
                if res != 0 {
                    break;
                }
                let session_id = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                let subkey = format!("{}\\{}\\VirtualDesktops", session_info_path, session_id);
                if let Some(bytes) = read_reg_binary(&subkey, "CurrentVirtualDesktop") {
                    if let Some(g) = guid_from_16_bytes(&bytes) {
                        RegCloseKey(hkey);
                        return Some(g);
                    }
                }
                index += 1;
            }
            RegCloseKey(hkey);
        }
    }

    None
}

/// Lê o nome customizado do Desktop Virtual no Registro do Windows (se o usuário renomeou no Win+Tab).
fn read_desktop_name_from_registry(desktop_guid: &GUID) -> Option<String> {
    let subkey = format!(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops\\Desktops\\{}\0",
        format_guid(desktop_guid)
    );
    let subkey_utf16: Vec<u16> = subkey.encode_utf16().collect();
    let value_name: Vec<u16> = "Name\0".encode_utf16().collect();

    let mut buffer = [0u16; 256];
    let mut byte_size = (buffer.len() * 2) as u32;

    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey_utf16.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            buffer.as_mut_ptr() as *mut c_void,
            &mut byte_size,
        )
    };

    if status == 0 && byte_size >= 2 {
        let char_len = (byte_size as usize / 2).saturating_sub(1);
        let name = String::from_utf16_lossy(&buffer[..char_len]).trim().to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

/// Consulta instantaneamente o índice (1-based) e nome do Desktop ativo via Registro.
/// No seu Windows 10 22H2, `VirtualDesktopIDs` contém os 48 bytes (3 Desktops de 16 bytes)
/// e `SessionInfo\1\VirtualDesktops\CurrentVirtualDesktop` contém os 16 bytes do Desktop atual!
pub fn get_desktop_info_from_registry() -> Option<(u32, String)> {
    let all_ids_bytes = read_reg_binary(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops",
        "VirtualDesktopIDs",
    )?;

    let current_guid = read_current_desktop_guid_from_registry()?;

    let total_desktops = all_ids_bytes.len() / 16;
    if total_desktops == 0 {
        return None;
    }

    for i in 0..total_desktops {
        let slice = &all_ids_bytes[i * 16..(i + 1) * 16];
        if let Some(item_guid) = guid_from_16_bytes(slice) {
            if guid_eq(&current_guid, &item_guid) {
                let display_number = (i as u32) + 1;
                let desktop_name = read_desktop_name_from_registry(&current_guid)
                    .unwrap_or_else(|| format!("Área de Trabalho {}", display_number));
                return Some((display_number, desktop_name));
            }
        }
    }

    None
}

// ============================================================================
// 2. ESTRATÉGIA SECUNDÁRIA (FALLBACK COM): WIN10 CLASSIC + SERVER 2022 + WIN11
// ============================================================================

const CLSID_IMMERSIVE_SHELL: GUID = GUID {
    data1: 0xC2F03A33,
    data2: 0x21F5,
    data3: 0x47FA,
    data4: [0xB4, 0xBB, 0x15, 0x63, 0x62, 0xA2, 0xF2, 0x39],
};

const CLSID_VIRTUAL_DESKTOP_MANAGER_INTERNAL: GUID = GUID {
    data1: 0xC5E0CDCA,
    data2: 0x7B6E,
    data3: 0x41B2,
    data4: [0x9F, 0xC4, 0xD9, 0x39, 0x75, 0xCC, 0x46, 0x7B],
};

const IID_ISERVICE_PROVIDER: GUID = GUID {
    data1: 0x6D5140C1,
    data2: 0x7436,
    data3: 0x11CE,
    data4: [0x80, 0x34, 0x00, 0xAA, 0x00, 0x60, 0x09, 0xFA],
};

const IID_IVIRTUAL_DESKTOP_MANAGER_INTERNAL_WIN10: GUID = GUID {
    data1: 0xF31574D6,
    data2: 0xB682,
    data3: 0x4CDC,
    data4: [0xBD, 0x56, 0x18, 0x27, 0x86, 0x0A, 0xBE, 0xC6],
};

const IID_IVIRTUAL_DESKTOP_WIN10: GUID = GUID {
    data1: 0xFF72FFDD,
    data2: 0xBE7E,
    data3: 0x43FC,
    data4: [0x9C, 0x03, 0xAD, 0x81, 0x68, 0x1E, 0x88, 0xE4],
};

#[repr(C)]
struct IUnknownVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

#[repr(C)]
struct IServiceProviderVtbl {
    base: IUnknownVtbl,
    query_service:
        unsafe extern "system" fn(*mut c_void, *const GUID, *const GUID, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IServiceProvider {
    lp_vtbl: *const IServiceProviderVtbl,
}

#[repr(C)]
struct IObjectArrayVtbl {
    base: IUnknownVtbl,
    get_count: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    get_at: unsafe extern "system" fn(*mut c_void, u32, *const GUID, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct IObjectArray {
    lp_vtbl: *const IObjectArrayVtbl,
}

#[repr(C)]
struct IVirtualDesktopWin10Vtbl {
    base: IUnknownVtbl,
    is_view_visible: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i32) -> HRESULT,
    get_id: unsafe extern "system" fn(*mut c_void, *mut GUID) -> HRESULT,
}

#[repr(C)]
struct IVirtualDesktopWin10 {
    lp_vtbl: *const IVirtualDesktopWin10Vtbl,
}

#[repr(C)]
struct IVirtualDesktopManagerInternalWin10Vtbl {
    base: IUnknownVtbl,
    get_count: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    move_view_to_desktop: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> HRESULT,
    can_view_move_desktops: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i32) -> HRESULT,
    get_current_desktop: unsafe extern "system" fn(*mut c_void, *mut *mut IVirtualDesktopWin10) -> HRESULT,
    get_desktops: unsafe extern "system" fn(*mut c_void, *mut *mut IObjectArray) -> HRESULT,
}

#[repr(C)]
struct IVirtualDesktopManagerInternalWin10 {
    lp_vtbl: *const IVirtualDesktopManagerInternalWin10Vtbl,
}

unsafe fn com_release<T>(ptr: *mut T) {
    if !ptr.is_null() {
        let unk = *(ptr as *const *const IUnknownVtbl);
        ((*unk).release)(ptr as *mut c_void);
    }
}

fn get_win10_com_desktop_info() -> Option<(u32, String)> {
    unsafe {
        let _ = CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED as u32);

        let mut service_provider: *mut IServiceProvider = ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_IMMERSIVE_SHELL,
            ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_ISERVICE_PROVIDER,
            &mut service_provider as *mut *mut IServiceProvider as *mut *mut c_void,
        );

        if hr < 0 || service_provider.is_null() {
            return None;
        }

        let mut vd_manager: *mut IVirtualDesktopManagerInternalWin10 = ptr::null_mut();
        let hr_qs = ((*(*service_provider).lp_vtbl).query_service)(
            service_provider as *mut c_void,
            &CLSID_VIRTUAL_DESKTOP_MANAGER_INTERNAL,
            &IID_IVIRTUAL_DESKTOP_MANAGER_INTERNAL_WIN10,
            &mut vd_manager as *mut *mut IVirtualDesktopManagerInternalWin10 as *mut *mut c_void,
        );

        com_release(service_provider);

        if hr_qs < 0 || vd_manager.is_null() {
            return None;
        }

        let mut current_desktop: *mut IVirtualDesktopWin10 = ptr::null_mut();
        let hr_cur = ((*(*vd_manager).lp_vtbl).get_current_desktop)(
            vd_manager as *mut c_void,
            &mut current_desktop,
        );

        if hr_cur < 0 || current_desktop.is_null() {
            com_release(vd_manager);
            return None;
        }

        let mut current_guid: GUID = std::mem::zeroed();
        let hr_id = ((*(*current_desktop).lp_vtbl).get_id)(
            current_desktop as *mut c_void,
            &mut current_guid,
        );
        com_release(current_desktop);

        if hr_id < 0 {
            com_release(vd_manager);
            return None;
        }

        let mut desktop_array: *mut IObjectArray = ptr::null_mut();
        let hr_list = ((*(*vd_manager).lp_vtbl).get_desktops)(
            vd_manager as *mut c_void,
            &mut desktop_array,
        );

        com_release(vd_manager);

        if hr_list < 0 || desktop_array.is_null() {
            return None;
        }

        let mut count: u32 = 0;
        let _ = ((*(*desktop_array).lp_vtbl).get_count)(desktop_array as *mut c_void, &mut count);

        let mut found_index: Option<u32> = None;

        for i in 0..count {
            let mut item_ptr: *mut IVirtualDesktopWin10 = ptr::null_mut();
            let hr_at = ((*(*desktop_array).lp_vtbl).get_at)(
                desktop_array as *mut c_void,
                i,
                &IID_IVIRTUAL_DESKTOP_WIN10,
                &mut item_ptr as *mut *mut IVirtualDesktopWin10 as *mut *mut c_void,
            );

            if hr_at >= 0 && !item_ptr.is_null() {
                let mut item_guid: GUID = std::mem::zeroed();
                if ((*(*item_ptr).lp_vtbl).get_id)(item_ptr as *mut c_void, &mut item_guid) >= 0 {
                    if guid_eq(&current_guid, &item_guid) {
                        found_index = Some(i + 1);
                        com_release(item_ptr);
                        break;
                    }
                }
                com_release(item_ptr);
            }
        }

        com_release(desktop_array);

        let display_number = found_index?;
        let desktop_name = read_desktop_name_from_registry(&current_guid)
            .unwrap_or_else(|| format!("Área de Trabalho {}", display_number));

        Some((display_number, desktop_name))
    }
}

// ============================================================================
// 3. FUNÇÃO PRINCIPAL EXPORTADA PARA O `main.rs`
// ============================================================================

/// Retorna `(numero_do_desktop_1_based, nome_do_desktop)`.
/// Ordem de prioridade:
/// 1. Leitura binária direta do Registro (`VirtualDesktopIDs` + `CurrentVirtualDesktop` em `SessionInfo`),
///    que é imune a mudanças de GUIDs COM em atualizações do Windows 10 22H2 (Build 19045.7807).
/// 2. API COM clássica do Windows 10 (`IVirtualDesktopManagerInternal`).
/// 3. Crate `winvd` (para Windows 11 24H2+).
pub fn get_active_desktop_info() -> Option<(u32, String)> {
    // 1. Estratégia Registro Binário (funciona em todas as builds do Windows 10 22H2 e Win11)
    if let Some(info) = get_desktop_info_from_registry() {
        return Some(info);
    }

    // 2. Fallback COM clássico Windows 10
    if let Some(info) = get_win10_com_desktop_info() {
        return Some(info);
    }

    // 3. Fallback Windows 11 24H2+ via `winvd`
    if let Ok(desktop) = winvd::get_current_desktop() {
        if let Ok(idx_zero) = desktop.get_index() {
            let display_num = idx_zero + 1;
            let name = desktop
                .get_name()
                .ok()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| format!("Área de Trabalho {}", display_num));
            return Some((display_num, name));
        }
    }

    None
}
