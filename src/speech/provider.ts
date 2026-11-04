/** Which speech engine this build talks to. The provider branches change this value. */
export const SPEECH_PROVIDER = "phonon" as "none" | "deepgram" | "phonon";

export const DEFAULT_PHONON_MODEL = "phonon-2.5-en";
export const DEFAULT_DEEPGRAM_MODEL = "nova-3";
