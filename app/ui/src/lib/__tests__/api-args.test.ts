import { describe, expect, it } from 'vitest';
import { startMeetingArgs } from '../api';

// The Tauri command `start_meeting(title, in_person)` expects camelCase arguments:
// `in_person` would be ignored and the meeting would start as a regular call.
describe('startMeetingArgs', () => {
  it('sends `inPerson` in camelCase', () => {
    expect(startMeetingArgs(undefined, true)).toEqual({ inPerson: true });
    expect(startMeetingArgs('Atelier', true)).toEqual({ title: 'Atelier', inPerson: true });
  });
  it('omits default arguments', () => {
    expect(startMeetingArgs()).toEqual({});
    expect(startMeetingArgs('Daily')).toEqual({ title: 'Daily' });
  });
});
