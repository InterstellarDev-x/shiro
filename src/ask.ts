import type { AudioInput, TranscriptLine, TranscriptSource } from "./hooks/useTranscript";

/** Finished interviewer line. Natively's profile-grounding / answer gate. */
export const GROUNDING_MIN_CONFIDENCE = 0.6;
/** Interim line, after it has sat still. Natively's speculative prefetch gate. */
export const SPECULATIVE_MIN_CONFIDENCE = 0.75;
export const SPECULATIVE_MIN_WORDS = 7;
export const SPECULATIVE_DEBOUNCE_MS = 350;
/** Reuse a speculative answer only when the finished line is the same ask. */
export const SPECULATIVE_SIMILARITY_THRESHOLD = 0.75;

const GREETING_ONLY =
  /^(?:hi|hello|hey|good (?:morning|afternoon|evening)|how are you|nice to meet you|thanks?|thank you|welcome|let'?s (?:get )?started|can you hear me|are you there)[\s!.,?]*$/i;

const QUESTION_MARK = /[?？]/;

const INTERROGATIVE_LEAD =
  /^(?:what|who|why|where|when|which|how|whose|whom|can|could|would|will|do|did|does|are|is|were|was|have|has|had|tell me|walk me|describe|explain|give me|share|let'?s talk about|talk about|i'?d like to (?:hear|know)|i want to (?:hear|know))\b/i;

const IMPERATIVE_ASK =
  /\b(?:tell me|walk me|describe|explain|give me|show me|share|talk about|let'?s talk about|i'?d like to (?:hear|know)|i want to (?:hear|know))\b/i;

const TASK_DIRECTIVE =
  /(?:^|[,.!?]\s*)(?:(?:ok(?:ay)?|so|now|next|alright|great|please)[,.!]?\s+)*(?:please\s+)?(?:solve|write|implement|rate|rank|convince|design|build|debug|code|optimi[sz]e|refactor|estimate|compare|sketch|whiteboard|connect|reverse|check|find|print|sort(?!\s+of\b)|merge|compute|calculate|invert|traverse)\b/i;

const HINDI_ASK =
  /(?:क्या|कैसे|क्यों|कब|कहाँ|कहां|कौन|कितना|कितने|बताइए|बताओ|बताइये|समझाइए|समझाओ|\b(?:kya|kaise|kyun|kyon|kab|kahan|kaun|kitna|kitne|batao|bataiye|samjhao|samjhaiye)\b)/i;

const SOCIAL_PLEASANTRY =
  /\b(?:trouble |any (?:trouble|problem)s? )?(?:finding|find) (?:the office|us|parking|the parking|your way|this place|the building)\b|\bfind (?:us|the office|parking|the building|your way|this place)\s+(?:ok(?:ay)?|alright|all right)\b|\bhow(?:\s+(?:was|is)|\s*['’]?s)\s+your\s+(?:weekend|day|morning|week|commute|drive|trip|flight)\b|\bhow(?:\s+are|\s*['’]?re)\s+you\s+(?:doing|feeling|holding up)\b|\bhow'?s the weather\b|\bdid you (?:get|grab|have) (?:any |some )?(?:coffee|water|tea|lunch)\b|\b(?:traffic|parking|weather|commute) (?:was|is|been)\b|\bhow was the (?:traffic|commute|drive|trip|flight|parking)\b/i;

const WAIT_IDIOM =
  /\b(?:give (?:me|us) (?:a|one|two|just a) (?:sec(?:ond)?s?|minutes?|moments?|mins?)\b(?!\s+(?:opinion|chance|example|reason|thought|look))|bear with me|hold on a (?:sec(?:ond)?|minute|moment)|one (?:moment|sec(?:ond)?),? please)\b/i;

const SPLIT_CONSTRAINT =
  /^(?:you\s+(?:can|may|should|could)\s+(?:safely\s+)?assume|assume\b|note\s+that|the\s+constraints?\b|constraints?\s*:|the\s+input\s+(?:is|will|can|may)|inputs?\s+(?:are|is|will)|do\s+it\s+(?:in|without)|without\s+using\b|in\s+o\s*\(|with\s+o\s*\()/i;

const CONSTRAINT_GAP_MS = 30_000;

const DEMONSTRATIVE_FOLLOW_UP =
  /\b(?:explain|elaborate on|tell me more about|go deeper into|expand on)\s+(?:that|this|it|those|these)\b/i;

const STRONG_FOLLOW_UP =
  /\b(?:you (?:just )?(?:said|mentioned)\b|\bmentioned (?:earlier|before)\b|\bsaid (?:earlier|before)\b|(?:going|coming) back to (?:that|this|it|what you (?:said|mentioned)|the (?:earlier|previous|last) (?:point|topic|question|thing))\b|the previous (?:point|topic|question|thing|example)(?: you (?:mentioned|said|brought up|raised))?\b|circling back|you (?:had )?(?:brought up|touched on|referenced))\b/i;

const WEAK_FOLLOW_UP =
  /\b(?:that|this|it|those|these|the (?:project|one|system|approach|role|company)|in more detail|more about (?:that|it|this)|elaborate|go deeper|expand on)\b/i;

const FOLLOW_UP_WORD_CAP = 14;

const QUESTION_WH = /\b(?:what|how|why|where|when|which|who|whose|whom)\b/i;
const QUESTION_LEAD =
  String.raw`(?:^|\b(?:so|and|but|ok|okay|uh|um|now|then|yeah|alright|right|well|also|just)\s+)`;
const QUESTION_AUX = new RegExp(
  QUESTION_LEAD +
    String.raw`(?:is|are|was|were|do|does|did|can|could|would|should|will|shall|have|has|had|am|isn't|aren't|don't|doesn't|didn't|can't|couldn't|wouldn't|shouldn't|won't|haven't|hasn't)\s+(?:you|it|that|this|there|we|they|he|she|i|your|the|a|an|any|anyone|anything|something|someone|everyone|all|each)\b`,
  "i",
);
const QUESTION_REQUEST = new RegExp(
  QUESTION_LEAD +
    String.raw`(?:tell me|explain|describe|walk (?:me|us) through|talk (?:me|us) through|show me|give me|let me know|help me|can you|could you|would you|will you|i'd like to (?:know|hear|see|understand)|i want to (?:know|hear|see|understand)|i'm curious|im curious|i wonder|wondering|any (?:thoughts|ideas|idea|questions|question)|what about|how about|remind me|clarify|elaborate|implement|write (?:a|an|the|some|me)|code (?:up|this|it)|optimi[sz]e|refactor|debug|fix (?:this|that|it|the)|summari[sz]e|recap|go over|run (?:me|us) through)\b`,
  "i",
);
const QUESTION_SHOULD = /\b(?:should (?:i|we|you|they|it)|shall (?:i|we))\b/i;

const STOP_WORDS = new Set([
  "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "am",
  "do", "does", "did", "have", "has", "had", "having",
  "what", "which", "who", "whom", "whose", "when", "where", "why", "how",
  "can", "could", "would", "should", "will", "shall", "may", "might", "must",
  "i", "me", "my", "mine", "we", "us", "our", "ours",
  "you", "your", "yours", "he", "him", "his", "she", "her", "hers",
  "it", "its", "they", "them", "their", "theirs",
  "and", "or", "but", "not", "no", "so", "if", "then", "than",
  "that", "this", "these", "those", "there", "here",
  "of", "in", "on", "at", "to", "for", "with", "from", "by", "as",
  "some", "any", "about", "please", "tell", "give", "let", "lets",
]);

const CONTENT_AGREEMENT_MIN = 0.6;
const CONTENT_SUBSTITUTION_CAP = 0.6;
const SPECULATION_COVERAGE_MIN = 0.9;

export interface AskDecision {
  source: TranscriptSource;
  text: string;
  keys: string[];
  confidence: number;
}

/** Mic answers you, System answers the other person, Both answers either. */
export function sourcesForInput(input: AudioInput): TranscriptSource[] {
  if (input === "mic") return ["me"];
  if (input === "system") return ["them"];
  return ["me", "them"];
}

function opened(text: string): string {
  return text
    .trim()
    .replace(
      /^(?:(?:um+|uh+|er+|ah+|hmm+|so|ok(?:ay)?|well|right|and|now|alright|great|yeah|yes|hi|hello|hey|thanks|thank you)[,.\s]+)+/i,
      "",
    )
    .trim();
}

function stamp(id: string): number {
  const value = Number(id.split("-")[0]);
  return Number.isFinite(value) ? value : 0;
}

function isGreeting(text: string): boolean {
  const raw = text.trim();
  return GREETING_ONLY.test(raw) || GREETING_ONLY.test(opened(raw));
}

function wordCount(text: string): number {
  return text.trim().split(/\s+/).filter(Boolean).length;
}

/** Speculation only. A finished line is gated by confidence, not this signal. */
export function hasQuestionSignal(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed) return false;
  return (
    trimmed.endsWith("?") ||
    trimmed.endsWith("？") ||
    QUESTION_WH.test(trimmed) ||
    QUESTION_AUX.test(trimmed) ||
    QUESTION_REQUEST.test(trimmed) ||
    QUESTION_SHOULD.test(trimmed) ||
    HINDI_ASK.test(trimmed)
  );
}

type QuestionType =
  | "identity"
  | "profile_detail"
  | "jd_alignment"
  | "negotiation"
  | "behavioral"
  | "technical"
  | "follow_up"
  | "general";

function classifyType(question: string): QuestionType {
  const text = question.toLowerCase();
  if (/\b(?:your (?:full |first |last )?name|who are you|what'?s your name|what is your name)\b/.test(text)) return "identity";
  if (/\b(?:introduce yourself|introducing yourself|tell me about yourself|describe yourself|about yourself)\b/.test(text)) return "identity";
  if (/\bself[- ]?(?:introduc(?:tion|e)|intro)\b/.test(text)) return "identity";
  if (/\b(?:give|giving|provide|share|start (?:us |me |the team )?off with)\b.{0,30}\b(?:a |an )?(?:quick |brief |short |little )?(?:introduction|intro)\b/.test(text)) return "identity";
  if (/\b(?:salary|compensation|comp|pay|package|ctc|equity|stock|bonus|offer|expectations? (?:for|on) (?:pay|salary|comp)|how much (?:do|are) you (?:expect|looking)|what are you (?:expecting|looking for)|notice period|joining date)\b/.test(text)) return "negotiation";
  if (/\b(?:good fit|right fit|why (?:should we|do you want|are you interested)|fit for (?:this|the) (?:role|position|job)|why this (?:role|company|position)|what makes you|why you)\b/.test(text)) return "jd_alignment";
  if (/\b(?:tell me about a time|describe a (?:situation|time)|give me an example of a time|when have you|a time when you|walk me through a (?:time|situation)|how did you handle|conflict|challenge you faced)\b/.test(text)) return "behavioral";
  if (/\b(?:your )?(?:projects?|side projects?|experience|work history|background|skills?|tech stack|education|degree|studied|university|college|achievements?|certifications?|what have you (?:built|worked on|done))\b/.test(text)) return "profile_detail";
  if (/\b(?:implement|write (?:code|a function|a program)|algorithm|data structure|system design|how does .* work|explain (?:how|the)|difference between|what is (?:a|an|the)|optimi[sz]e|debug|complexity)\b/.test(text)) return "technical";
  return "general";
}

function scoreAskShape(hasMark: boolean, hasLead: boolean): number | null {
  if (hasMark && hasLead) return 0.95;
  if (hasMark || hasLead) return 0.8;
  return null;
}

interface ScoredTurn {
  text: string;
  keys: string[];
  confidence: number;
}

function scoreTurn(
  source: TranscriptSource,
  raw: string,
  previous: TranscriptLine | null,
  currentStamp: number,
  priorExists: boolean,
): ScoredTurn {
  let scoring = opened(raw);
  let emitted = raw.trim();
  const keys = [`${source}:${raw.trim()}`];

  if (
    previous &&
    previous.source === source &&
    !QUESTION_MARK.test(emitted) &&
    SPLIT_CONSTRAINT.test(scoring) &&
    withinGap(stamp(previous.id), currentStamp)
  ) {
    const previousRaw = previous.text.trim();
    const previousScoring = opened(previousRaw);
    const previousIsAsk =
      QUESTION_MARK.test(previousRaw) ||
      INTERROGATIVE_LEAD.test(previousScoring) ||
      TASK_DIRECTIVE.test(previousScoring) ||
      HINDI_ASK.test(previousRaw);
    if (previousIsAsk && !isGreeting(previousRaw)) {
      scoring = `${previousScoring} ${scoring}`;
      emitted = `${previousRaw} ${emitted}`;
      keys.push(`${source}:${previousRaw}`);
    }
  }

  const hasMark = QUESTION_MARK.test(scoring) || QUESTION_MARK.test(emitted);
  const hasLead = INTERROGATIVE_LEAD.test(scoring) || HINDI_ASK.test(scoring) || HINDI_ASK.test(emitted);
  const hasTask = TASK_DIRECTIVE.test(scoring);
  const answerable =
    hasMark ||
    hasLead ||
    hasTask ||
    IMPERATIVE_ASK.test(scoring) ||
    DEMONSTRATIVE_FOLLOW_UP.test(scoring) ||
    STRONG_FOLLOW_UP.test(scoring);

  const followUp =
    priorExists &&
    answerable &&
    (DEMONSTRATIVE_FOLLOW_UP.test(scoring) ||
      STRONG_FOLLOW_UP.test(scoring) ||
      (WEAK_FOLLOW_UP.test(scoring) && wordCount(scoring) <= FOLLOW_UP_WORD_CAP));

  const questionType: QuestionType = followUp ? "follow_up" : classifyType(scoring);
  let confidence = 0.4;
  const core = scoreAskShape(hasMark, hasLead);
  if (core !== null) confidence = core;
  else if (hasTask) confidence = 0.75;
  if (questionType !== "general" && confidence < 0.8) confidence = 0.7;
  const nonSubstantive = questionType === "general" || questionType === "follow_up";
  if (SOCIAL_PLEASANTRY.test(scoring) && nonSubstantive) confidence = Math.min(confidence, 0.5);
  if (WAIT_IDIOM.test(scoring) && nonSubstantive) confidence = Math.min(confidence, 0.5);
  if (!answerable) confidence = Math.min(confidence, 0.3);

  return { text: emitted, keys, confidence };
}

function withinGap(previousStamp: number, currentStamp: number): boolean {
  if (!previousStamp || !currentStamp) return true;
  return Math.abs(currentStamp - previousStamp) <= CONSTRAINT_GAP_MS;
}

/**
 * The newest non-greeting line from the selected inputs, scored the way
 * Natively scores an interviewer turn. A line below the gate is not an ask,
 * and an older line is not considered.
 */
export function latestAsk(
  lines: TranscriptLine[],
  gate: number,
  sources: readonly TranscriptSource[],
  pending?: { source: TranscriptSource; text: string },
): AskDecision | null {
  const turns: TranscriptLine[] = [...lines];
  const pendingText = pending?.text.trim();
  if (pending && pendingText && sources.includes(pending.source)) {
    turns.push({ id: `${Date.now()}-partial`, source: pending.source, text: pendingText });
  }

  let chosen = -1;
  for (let index = turns.length - 1; index >= 0; index -= 1) {
    const line = turns[index];
    if (!sources.includes(line.source)) continue;
    const text = line.text.trim();
    if (!text || isGreeting(text)) continue;
    chosen = index;
    break;
  }
  if (chosen < 0) return null;

  const current = turns[chosen];
  const previous = chosen > 0 ? turns[chosen - 1] : null;
  const priorExists = turns.slice(0, chosen).some((line) => line.text.trim() && !isGreeting(line.text));
  const scored = scoreTurn(current.source, current.text, previous, stamp(current.id), priorExists);
  if (scored.confidence < gate) return null;
  return { source: current.source, text: scored.text, keys: scored.keys, confidence: scored.confidence };
}

/** Finished line from the selected input at the 0.6 gate. */
export function groundedAsk(lines: TranscriptLine[], input: AudioInput = "both"): AskDecision | null {
  return latestAsk(lines, GROUNDING_MIN_CONFIDENCE, sourcesForInput(input));
}

/**
 * Live partial from the selected input with a question signal, at least seven
 * words, and confidence of at least 0.75. The caller waits
 * {@link SPECULATIVE_DEBOUNCE_MS} so a moving partial does not start an answer
 * on every word.
 */
export function speculativeAsk(
  lines: TranscriptLine[],
  partial: string,
  source: TranscriptSource,
  input: AudioInput = "both",
): AskDecision | null {
  const text = partial.trim();
  if (!sourcesForInput(input).includes(source)) return null;
  if (wordCount(text) < SPECULATIVE_MIN_WORDS) return null;
  if (!hasQuestionSignal(text)) return null;
  return latestAsk(lines, SPECULATIVE_MIN_CONFIDENCE, sourcesForInput(input), { source, text });
}

function wordsOf(text: string): Set<string> {
  return new Set(text.toLowerCase().match(/\b\w+\b/g) ?? []);
}

function contentWordsOf(text: string): Set<string> {
  const content = new Set<string>();
  for (const word of wordsOf(text)) {
    if (!STOP_WORDS.has(word)) content.add(word);
  }
  return content;
}

function blended(left: Set<string>, right: Set<string>): number {
  if (left.size === 0 && right.size === 0) return 1;
  let shared = 0;
  for (const word of left) if (right.has(word)) shared += 1;
  const jaccard = shared / (left.size + right.size - shared);
  const containment = left.size > 0 ? shared / left.size : 0;
  return Math.max(jaccard, containment * 0.9);
}

export function speculativeQuestionSimilarity(speculative: string, finalText: string): number {
  const raw = blended(wordsOf(speculative), wordsOf(finalText));
  const contentA = contentWordsOf(speculative);
  const contentB = contentWordsOf(finalText);
  if (contentA.size === 0 || contentB.size === 0) return raw;
  const content = blended(contentA, contentB);
  const meaningful = (word: string) => word.length >= 3;
  let partialHasExclusive = false;
  let finalHasExclusive = false;
  for (const word of contentA) {
    if (meaningful(word) && !contentB.has(word)) partialHasExclusive = true;
  }
  for (const word of contentB) {
    if (meaningful(word) && !contentA.has(word)) finalHasExclusive = true;
  }
  if (partialHasExclusive && finalHasExclusive) return Math.min(raw, content, CONTENT_SUBSTITUTION_CAP);
  return content < CONTENT_AGREEMENT_MIN ? Math.min(raw, content) : raw;
}

export function speculationCoversQuestion(speculative: string, candidate: string): boolean {
  const want = contentWordsOf(candidate);
  if (want.size === 0) return false;
  const have = contentWordsOf(speculative);
  let covered = 0;
  for (const word of want) if (have.has(word)) covered += 1;
  return covered / want.size >= SPECULATION_COVERAGE_MIN;
}

/** The finished line is the question the speculative answer already heard. */
export function canAdoptSpeculation(speculative: string, finalText: string): boolean {
  return (
    speculativeQuestionSimilarity(speculative, finalText) >= SPECULATIVE_SIMILARITY_THRESHOLD &&
    speculationCoversQuestion(speculative, finalText)
  );
}
