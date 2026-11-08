import type { DeepString } from "../types";
import type { en } from "./en";

export const pl: DeepString<typeof en> = {
  tabs: {
    telemetry: "Telemetria",
    lighting: "Oświetlenie",
    device: "Urządzenie",
  },

  stream: {
    idle: "brak połączenia",
    connecting: "łączenie…",
    live: "na żywo",
    reconnecting: "ponowne łączenie…",
    unauthorized: "token odrzucony",
  },

  nav: {
    dashboard: "pulpit",
    lighting: "oświetlenie",
    history: "historia",
    settings: "ustawienia",
  },
  common: {
    on: "wł.",
    off: "wył.",
    yes: "tak",
    no: "nie",
    ok: "ok",
    idle: "bezczynny",
    healthy: "sprawne",
    sensorsOff: "czujniki wyłączone",
    noDevice: "nie skonfigurowano urządzenia",
    noData: "brak danych",
    close: "zamknij",
    cancel: "anuluj",
    appName: "akwarium",
  },

  deviceForm: {
    host: "adres",
    port: "port",
    token: "token",
    portHelper: "domyślnie 80",
    submit: "zapisz i połącz",
    cannotConnect: "nie można połączyć",
    tokenRejected: "urządzenie odrzuciło ten token",
    unreachable: "brak kontaktu z {{host}}:{{port}}",
  },

  validation: {
    hostRequired: "podaj adres urządzenia",
    hostNoScheme: "bez schematu, sam adres lub ip",
    portNumber: "port musi być liczbą",
    portRange: "port musi mieścić się w zakresie 1–65535",
    tokenRequired: "podaj token api",
  },

  language: {
    label: "język",
    en: "English",
    pl: "Polski",
  },

  telemetry: {
    loading: "pobieranie odczytu…",
    failedTitle: "błąd telemetrii",
    unauthorizedTitle: "token odrzucony",
    unauthorizedBody:
      "urządzenie odrzuciło token api — otwórz zakładkę Urządzenie i wpisz go ponownie",
    unreachableTitle: "urządzenie nieosiągalne",
    unreachableBody:
      "brak odpowiedzi od sterownika — może być wyłączony albo w innej sieci",
    malformedTitle: "nieoczekiwana telemetria",
    errorTitle: "urządzenie zwróciło błąd",
    leakTitle: "wykryto wyciek",
    leakBody: "woda poza akwarium — sprawdź sump i podłogę",
  },

  lighting: {
    loading: "odczyt oświetlenia…",
    now: "teraz",
    override: "nadpisanie",
    onSchedule: "wg harmonogramu",
    sending: "wysyłanie…",
    ready: "gotowe",
    schedulePaused: "nadpisanie aktywne — harmonogram wstrzymany",
    scheduleRunning: "harmonogram działa",
    restore: "przywróć harmonogram",
    refused: "urządzenie to odrzuciło",
    duration: "czas trwania",
    profile: "profil",
    manualChannels: "kanały ręczne",
    white: "biały",
    rgb: "rgb",
    whiteRgb: "biały / rgb",
    expiresIn: "wygasa za",
  },

  durations: {
    m5: "5 min",
    m15: "15 min",
    h1: "1 godz.",
    none: "bez limitu",
  },

  profiles: {
    off: "wyłączony",
    sunrise: "wschód",
    daylight: "dzień",
    sunset: "zachód",
    moonlight: "księżyc",
  },

  systemModes: {
    normal: "normalny",
  },

  cards: {
    temperature: {
      title: "temperatura",
      target: "docelowa",
      heater: "grzałka",
      sump: "sump",
    },
    filtration: {
      title: "filtracja",
      flowing: "przepływ",
      noFlow: "brak przepływu",
      flow: "przepływ",
      relay: "przekaźnik",
    },
    co2: {
      title: "co2",
      dosing: "dozowanie",
      ph: "pH",
      target: "docelowe",
      solenoid: "elektrozawór",
      electrode: "elektroda",
    },
    ato: {
      title: "ato",
      reservoirEmpty: "zbiornik pusty",
      sumpLow: "niski poziom w sumpie",
      filling: "napełnianie",
      pump: "pompa",
      reservoir: "zbiornik",
      empty: "pusty",
      lastFill: "ostatnie napełnienie",
    },
    lighting: {
      title: "oświetlenie",
      override: "nadpisanie",
      profile: "profil",
      whiteRgb: "biały / rgb",
    },
    system: {
      title: "system",
      wifi: "wifi",
      freeHeap: "wolna pamięć",
      uptime: "czas pracy",
    },
  },

  water: {
    title: "poziom wody",
    faultyOne: "1 czujnik uszkodzony",
    faultyMany: "uszkodzonych czujników: {{count}}",
    mainTank: "akwarium",
    mechanical: "mechaniczna",
    biological: "biologiczna",
    pump: "pompowa",
    reserve: "rezerwa",
  },
};
