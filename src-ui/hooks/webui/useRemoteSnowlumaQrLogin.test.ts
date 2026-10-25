// @vitest-environment jsdom
import { act, renderHook, waitFor } from '@testing-library/react';
import { createElement, StrictMode, type ReactNode } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
    SL_QR_REQUEST_TIMEOUT_MS,
    SL_QR_SESSION_TIMEOUT_MS,
    useRemoteSnowlumaQrLogin,
    type RemoteQrLoginApi,
} from './useRemoteSnowlumaQrLogin';
import type { SnowLumaQrLoginSnapshot } from '../../core/ipc/generated/domain/SnowLumaQrLoginSnapshot';

const snapshot: SnowLumaQrLoginSnapshot = {
    v: 1,
    state: 'waiting_for_qr',
    qq_pid: 19,
    qrcode_payload: 'https://txz.qq.com/p?k=test',
    uin: null,
    observer_removed: false,
    message: null,
};

describe('remote SL QR lifecycle', () => {
    afterEach(() => {
        vi.useRealTimers();
    });

    it('automatically replaces each expired code and only polls a valid code', async () => {
        vi.useFakeTimers();
        const expired = { ...snapshot, state: 'expired' as const, qrcode_payload: null };
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockResolvedValueOnce(expired)
                .mockResolvedValueOnce({ ...snapshot, qrcode_payload: 'new-code-1' })
                .mockResolvedValueOnce(expired)
                .mockResolvedValue({ ...snapshot, qrcode_payload: 'new-code-2' }),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        await act(async () => vi.advanceTimersByTimeAsync(1000));
        expect(result.current.snapshot?.qrcode_payload).toBe('new-code-1');
        await act(async () => vi.advanceTimersByTimeAsync(1000));
        expect(result.current.snapshot?.state).toBe('expired');
        await act(async () => vi.advanceTimersByTimeAsync(1000));
        expect(result.current.snapshot?.qrcode_payload).toBe('new-code-2');
        await act(async () => vi.advanceTimersByTimeAsync(1000));
        expect(vi.mocked(api.qrLoginSnapshot).mock.calls.map(([, refresh]) => refresh)).toEqual([
            false,
            true,
            false,
            true,
            false,
        ]);
        unmount();
    });

    it('backs off unsuccessful expiry refreshes and cancels the pending refresh on close', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => ({
                ...snapshot,
                state: 'expired' as const,
                qrcode_payload: null,
            })),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { rerender, unmount } = renderHook(
            ({ open }) => useRemoteSnowlumaQrLogin('123456', open, api),
            { initialProps: { open: true } },
        );
        await act(async () => {});
        await act(async () => vi.advanceTimersByTimeAsync(1000));
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(2);
        await act(async () => vi.advanceTimersByTimeAsync(1999));
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(2);
        await act(async () => vi.advanceTimersByTimeAsync(1));
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(3);
        rerender({ open: false });
        await act(async () => vi.advanceTimersByTimeAsync(10000));
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(3);
        unmount();
    });

    it('stops automatic refresh once QQ reports phone confirmation', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockResolvedValueOnce({ ...snapshot, state: 'expired' })
                .mockResolvedValueOnce({ ...snapshot, state: 'scanned' })
                .mockResolvedValueOnce({ ...snapshot, state: 'login_detected' })
                .mockResolvedValue({ ...snapshot, state: 'expired' }),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        await act(async () => vi.advanceTimersByTimeAsync(5000));
        expect(result.current.snapshot?.state).toBe('login_detected');
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        expect(
            vi.mocked(api.qrLoginSnapshot).mock.calls.filter(([, refresh]) => refresh),
        ).toHaveLength(1);
        unmount();
    });

    it('recovers a transient capture failure by polling without refreshing a valid code', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockResolvedValueOnce({
                    ...snapshot,
                    state: 'starting',
                    qrcode_payload: null,
                    observer_removed: true,
                })
                .mockResolvedValue({ ...snapshot, observer_removed: true }),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() =>
            useRemoteSnowlumaQrLogin('123456', true, api, true),
        );
        await act(async () => {});
        expect(result.current.snapshot?.state).toBe('starting');
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1000);
        });
        expect(result.current.snapshot?.qrcode_payload).toBe(snapshot.qrcode_payload);
        expect(api.qrLoginSnapshot).toHaveBeenLastCalledWith('123456', false);
        expect(api.closeQrLogin).not.toHaveBeenCalled();
        unmount();
    });

    it.each(['unavailable', 'cancelled'] as const)(
        'explicit retry renews a %s session without closing the new request',
        async (state) => {
            const api: RemoteQrLoginApi = {
                qrLoginSnapshot: vi
                    .fn()
                    .mockResolvedValueOnce({
                        ...snapshot,
                        state,
                        qrcode_payload: null,
                        observer_removed: true,
                    })
                    .mockResolvedValue({ ...snapshot, observer_removed: true }),
                closeQrLogin: vi.fn(async () => {}),
            };
            const { result, unmount } = renderHook(() =>
                useRemoteSnowlumaQrLogin('123456', true, api),
            );
            await waitFor(() => expect(result.current.canRetry).toBe(true));
            act(() => result.current.retry());
            await waitFor(() =>
                expect(result.current.snapshot?.qrcode_payload).toBe(snapshot.qrcode_payload),
            );
            expect(api.qrLoginSnapshot).toHaveBeenLastCalledWith('123456', true);
            expect(api.closeQrLogin).not.toHaveBeenCalled();
            unmount();
        },
    );

    it('does not retry when a failed request has not confirmed cleanup', async () => {
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => ({
                ...snapshot,
                state: 'cleanup_failed' as const,
                qrcode_payload: null,
            })),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await waitFor(() => expect(result.current.snapshot?.state).toBe('cleanup_failed'));
        expect(result.current.canRetry).toBe(false);
        act(() => result.current.retry());
        expect(api.qrLoginSnapshot).toHaveBeenCalledOnce();
        unmount();
    });

    it('allows an unavailable session to retry only after its observer is confirmed closed', async () => {
        let finishClose!: () => void;
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => ({
                ...snapshot,
                state: 'unavailable' as const,
                qrcode_payload: null,
            })),
            closeQrLogin: vi.fn(
                () =>
                    new Promise<void>((resolve) => {
                        finishClose = resolve;
                    }),
            ),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await waitFor(() => expect(api.closeQrLogin).toHaveBeenCalledOnce());
        expect(result.current.canRetry).toBe(false);
        await act(async () => finishClose());
        expect(result.current.snapshot?.observer_removed).toBe(true);
        expect(result.current.canRetry).toBe(true);
        unmount();
    });
    it('switches the local login page only on initial open and polls without refreshing it', async () => {
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => snapshot),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api, true));
        await waitFor(() => expect(api.qrLoginSnapshot).toHaveBeenCalledWith('123456', true));
        await waitFor(() => expect(api.qrLoginSnapshot).toHaveBeenCalledWith('123456', false), {
            timeout: 2500,
        });
        expect(
            vi.mocked(api.qrLoginSnapshot).mock.calls.filter(([, refresh]) => refresh).length,
        ).toBe(1);
        unmount();
    });

    it('discards a late QR response after closing and closes the remote observer', async () => {
        let resolve!: (value: SnowLumaQrLoginSnapshot) => void;
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(
                () =>
                    new Promise((done) => {
                        resolve = done;
                    }),
            ),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, rerender, unmount } = renderHook(
            ({ open }) => useRemoteSnowlumaQrLogin('123456', open, api),
            { initialProps: { open: true } },
        );
        await waitFor(() => expect(api.qrLoginSnapshot).toHaveBeenCalledOnce());
        rerender({ open: false });
        await act(async () => {
            resolve(snapshot);
        });
        expect(result.current.snapshot).toBeNull();
        expect(api.closeQrLogin).toHaveBeenCalledWith('123456');
        unmount();
    });

    it('does not automatically request a new QR and clears terminal login material', async () => {
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => snapshot),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await waitFor(() => expect(result.current.snapshot?.state).toBe('waiting_for_qr'));
        expect(api.qrLoginSnapshot).toHaveBeenCalledWith('123456', false);
        vi.mocked(api.qrLoginSnapshot).mockResolvedValue({
            ...snapshot,
            state: 'logged_in',
            qrcode_payload: null,
            observer_removed: true,
        });
        await act(async () => {
            await result.current.refresh();
        });
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        expect(api.qrLoginSnapshot).toHaveBeenLastCalledWith('123456', true);
        unmount();
    });

    it('keeps login detection in status polling and drops residual terminal material', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockResolvedValueOnce({ ...snapshot, state: 'login_detected' })
                .mockResolvedValueOnce({ ...snapshot, state: 'starting' })
                .mockResolvedValue({ ...snapshot, state: 'logged_in', observer_removed: true }),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        expect(result.current.snapshot?.state).toBe('login_detected');
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        await act(async () => {
            await result.current.refresh();
        });
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(1);
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1000);
        });
        expect(result.current.snapshot?.state).toBe('login_detected');
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1000);
        });
        expect(result.current.snapshot?.state).toBe('logged_in');
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        await act(async () => {
            await vi.advanceTimersByTimeAsync(5000);
        });
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(3);
        unmount();
    });

    it.each(['scanned', 'login_detected'] as const)(
        'releases %s confirmation when QQ changes process without closing the dialog',
        async (state) => {
            vi.useFakeTimers();
            const api: RemoteQrLoginApi = {
                qrLoginSnapshot: vi
                    .fn()
                    .mockResolvedValueOnce({ ...snapshot, state })
                    .mockResolvedValueOnce({
                        ...snapshot,
                        state: 'starting',
                        qq_pid: 20,
                    })
                    .mockResolvedValue({
                        ...snapshot,
                        qq_pid: 20,
                        qrcode_payload: 'new-process-code',
                    }),
                closeQrLogin: vi.fn(async () => {}),
            };
            const { result, unmount } = renderHook(() =>
                useRemoteSnowlumaQrLogin('123456', true, api, true),
            );
            await act(async () => {});
            expect(result.current.snapshot?.state).toBe(state);
            expect(result.current.snapshot?.qrcode_payload).toBeNull();
            await act(async () => vi.advanceTimersByTimeAsync(1000));
            expect(result.current.snapshot?.state).toBe('starting');
            expect(result.current.snapshot?.qq_pid).toBe(20);
            expect(result.current.snapshot?.qrcode_payload).toBeNull();
            await act(async () => vi.advanceTimersByTimeAsync(1000));
            expect(result.current.snapshot?.state).toBe('waiting_for_qr');
            expect(result.current.snapshot?.qrcode_payload).toBe('new-process-code');
            await act(async () => {
                await result.current.refresh();
            });
            expect(api.qrLoginSnapshot).toHaveBeenLastCalledWith('123456', true);
            expect(api.closeQrLogin).not.toHaveBeenCalled();
            unmount();
        },
    );

    it('keeps confirmation when a transient response has no QQ process identity', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockResolvedValueOnce({ ...snapshot, state: 'login_detected' })
                .mockResolvedValueOnce({ ...snapshot, state: 'starting', qq_pid: null })
                .mockResolvedValue(snapshot),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        await act(async () => vi.advanceTimersByTimeAsync(2000));
        expect(result.current.snapshot?.state).toBe('login_detected');
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        await act(async () => {
            await result.current.refresh();
        });
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(3);
        unmount();
    });

    it('does not refresh or restore a stale QR while the phone confirmation is pending', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockResolvedValueOnce({ ...snapshot, state: 'scanned' })
                .mockResolvedValue(snapshot),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        await act(async () => {
            await result.current.refresh();
        });
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(1);
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1000);
        });
        expect(result.current.snapshot?.state).toBe('scanned');
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        unmount();
    });

    it('keeps phone confirmation stable across a transient starting probe', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockResolvedValueOnce({ ...snapshot, state: 'scanned' })
                .mockResolvedValue({ ...snapshot, state: 'starting' }),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1000);
        });
        expect(result.current.snapshot?.state).toBe('scanned');
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        await act(async () => {
            await result.current.refresh();
        });
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(2);
        unmount();
    });

    it('pauses requests and clears the visible QR while the page is hidden', async () => {
        vi.useFakeTimers();
        const visibility = Object.getOwnPropertyDescriptor(document, 'visibilityState');
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => snapshot),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() =>
            useRemoteSnowlumaQrLogin('123456', true, api, true),
        );
        try {
            await act(async () => {});
            await act(async () => {
                Object.defineProperty(document, 'visibilityState', {
                    configurable: true,
                    value: 'hidden',
                });
                document.dispatchEvent(new Event('visibilitychange'));
            });
            expect(result.current.paused).toBe(true);
            expect(result.current.snapshot).toBeNull();
            await act(async () => {
                await vi.advanceTimersByTimeAsync(10000);
            });
            expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(1);
            await act(async () => {
                Object.defineProperty(document, 'visibilityState', {
                    configurable: true,
                    value: 'visible',
                });
                document.dispatchEvent(new Event('visibilitychange'));
            });
            expect(api.qrLoginSnapshot).toHaveBeenLastCalledWith('123456', false);
        } finally {
            unmount();
            if (visibility) Object.defineProperty(document, 'visibilityState', visibility);
            else Reflect.deleteProperty(document, 'visibilityState');
        }
    });

    it('retries a settled channel failure without repeating the refresh action', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockRejectedValueOnce(new Error('temporary tunnel failure'))
                .mockResolvedValue(snapshot),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() =>
            useRemoteSnowlumaQrLogin('123456', true, api, true),
        );
        await act(async () => {});
        expect(result.current.recovering).toBe(true);
        expect(result.current.snapshot?.qrcode_payload).toBeUndefined();
        await act(async () => vi.advanceTimersByTimeAsync(2000));
        expect(api.qrLoginSnapshot).toHaveBeenNthCalledWith(1, '123456', true);
        expect(api.qrLoginSnapshot).toHaveBeenNthCalledWith(2, '123456', false);
        expect(result.current.snapshot?.qrcode_payload).toBe(snapshot.qrcode_payload);
        expect(result.current.error).toBeNull();
        expect(result.current.recovering).toBe(false);
        expect(api.closeQrLogin).not.toHaveBeenCalled();
        unmount();
    });

    it('bounds automatic retries and waits for cleanup before offering another attempt', async () => {
        vi.useFakeTimers();
        let finishClose!: () => void;
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => {
                throw new Error('channel unavailable');
            }),
            closeQrLogin: vi.fn(
                () =>
                    new Promise<void>((resolve) => {
                        finishClose = resolve;
                    }),
            ),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        await act(async () => vi.advanceTimersByTimeAsync(6000));
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(3);
        expect(api.closeQrLogin).toHaveBeenCalledOnce();
        expect(result.current.canRetry).toBe(false);
        await act(async () => finishClose());
        expect(result.current.canRetry).toBe(true);
        await act(async () => vi.advanceTimersByTimeAsync(10000));
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(3);
        unmount();
    });

    it('removes a stale code on channel failure and preserves phone confirmation on recovery', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockResolvedValueOnce(snapshot)
                .mockRejectedValueOnce(new Error('temporary tunnel failure'))
                .mockResolvedValueOnce({ ...snapshot, state: 'scanned' })
                .mockRejectedValueOnce(new Error('temporary tunnel failure'))
                .mockResolvedValue(snapshot),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        expect(result.current.snapshot?.qrcode_payload).toBe(snapshot.qrcode_payload);
        await act(async () => vi.advanceTimersByTimeAsync(1000));
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        await act(async () => vi.advanceTimersByTimeAsync(2000));
        expect(result.current.snapshot?.state).toBe('scanned');
        await act(async () => vi.advanceTimersByTimeAsync(3000));
        expect(result.current.snapshot?.state).toBe('scanned');
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        expect(result.current.error).toBeNull();
        expect(vi.mocked(api.qrLoginSnapshot).mock.calls.every(([, refresh]) => !refresh)).toBe(
            true,
        );
        unmount();
    });

    it('times out a hung request and waits for its end before allowing an in-place retry', async () => {
        vi.useFakeTimers();
        let resolve!: (value: SnowLumaQrLoginSnapshot) => void;
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(
                () =>
                    new Promise((done) => {
                        resolve = done;
                    }),
            ),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        await act(async () => {
            await vi.advanceTimersByTimeAsync(SL_QR_REQUEST_TIMEOUT_MS);
        });
        expect(result.current.error).toContain('未响应');
        expect(api.closeQrLogin).toHaveBeenCalledOnce();
        expect(result.current.canRetry).toBe(false);
        expect(result.current.snapshot).toBeNull();
        await act(async () => {
            resolve(snapshot);
        });
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        expect(result.current.canRetry).toBe(true);
        vi.mocked(api.qrLoginSnapshot).mockResolvedValue({
            ...snapshot,
            qrcode_payload: 'fresh-code',
        });
        await act(async () => result.current.retry());
        expect(result.current.snapshot?.qrcode_payload).toBe('fresh-code');
        expect(api.qrLoginSnapshot).toHaveBeenLastCalledWith('123456', true);
        unmount();
    });

    it('reports a failed cleanup without retaining any login material', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => {
                throw new Error('unavailable');
            }),
            closeQrLogin: vi.fn(async () => {
                throw new Error('close failed');
            }),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        await act(async () => vi.advanceTimersByTimeAsync(6000));
        expect(result.current.snapshot?.state).toBe('cleanup_failed');
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        expect(result.current.snapshot?.observer_removed).toBe(false);
        expect(result.current.error).toBeNull();
        unmount();
    });

    it('does not offer retry when closing a timed-out request also hangs', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(() => new Promise<SnowLumaQrLoginSnapshot>(() => {})),
            closeQrLogin: vi.fn(() => new Promise<void>(() => {})),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        await act(async () => vi.advanceTimersByTimeAsync(SL_QR_REQUEST_TIMEOUT_MS * 2));
        expect(result.current.snapshot?.state).toBe('cleanup_failed');
        expect(result.current.canRetry).toBe(false);
        expect(result.current.recovering).toBe(false);
        unmount();
    });

    it.each(['scanned', 'login_detected'] as const)(
        'does not restart a %s login after a timed-out status request is closed',
        async (state) => {
            vi.useFakeTimers();
            let finishOld!: (value: SnowLumaQrLoginSnapshot) => void;
            const api: RemoteQrLoginApi = {
                qrLoginSnapshot: vi
                    .fn()
                    .mockResolvedValueOnce({ ...snapshot, state })
                    .mockImplementationOnce(
                        () =>
                            new Promise<SnowLumaQrLoginSnapshot>((resolve) => {
                                finishOld = resolve;
                            }),
                    ),
                closeQrLogin: vi.fn(async () => {}),
            };
            const { result, unmount } = renderHook(() =>
                useRemoteSnowlumaQrLogin('123456', true, api),
            );
            await act(async () => {});
            await act(async () => vi.advanceTimersByTimeAsync(1000 + SL_QR_REQUEST_TIMEOUT_MS));
            await act(async () => finishOld(snapshot));
            expect(result.current.snapshot?.qrcode_payload).toBeNull();
            expect(result.current.canRetry).toBe(false);
            await act(async () => result.current.retry());
            expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(2);
            unmount();
        },
    );

    it('does not overwrite a reopened confirmed login with an old QR response', async () => {
        let resolveOld!: (value: SnowLumaQrLoginSnapshot) => void;
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockImplementationOnce(
                    () =>
                        new Promise<SnowLumaQrLoginSnapshot>((done) => {
                            resolveOld = done;
                        }),
                )
                .mockResolvedValue({ ...snapshot, state: 'logged_in', observer_removed: true }),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, rerender, unmount } = renderHook(
            ({ open }) => useRemoteSnowlumaQrLogin('123456', open, api),
            { initialProps: { open: true } },
        );
        await waitFor(() => expect(api.qrLoginSnapshot).toHaveBeenCalledOnce());
        rerender({ open: false });
        await act(async () => {});
        rerender({ open: true });
        await act(async () => {});
        expect(api.qrLoginSnapshot).toHaveBeenCalledOnce();
        expect(result.current.recovering).toBe(true);
        await act(async () => {
            resolveOld(snapshot);
        });
        expect(result.current.snapshot?.state).toBe('logged_in');
        expect(result.current.snapshot?.qrcode_payload).toBeNull();
        unmount();
    });

    it.each(['close', 'request'] as const)(
        'waits for the old %s barrier after hiding and restoring a timed-out dialog',
        async (pendingBarrier) => {
            vi.useFakeTimers();
            const visibility = vi.spyOn(document, 'visibilityState', 'get');
            let finishOld!: (value: SnowLumaQrLoginSnapshot) => void;
            let finishClose!: () => void;
            const api: RemoteQrLoginApi = {
                qrLoginSnapshot: vi
                    .fn()
                    .mockImplementationOnce(
                        () =>
                            new Promise<SnowLumaQrLoginSnapshot>((resolve) => {
                                finishOld = resolve;
                            }),
                    )
                    .mockResolvedValue({ ...snapshot, qrcode_payload: 'resumed-code' }),
                closeQrLogin: vi.fn(
                    () =>
                        new Promise<void>((resolve) => {
                            finishClose = resolve;
                        }),
                ),
            };
            const { result, unmount } = renderHook(() =>
                useRemoteSnowlumaQrLogin('123456', true, api),
            );
            try {
                await act(async () => {});
                await act(async () => vi.advanceTimersByTimeAsync(SL_QR_REQUEST_TIMEOUT_MS));
                await act(async () => {
                    if (pendingBarrier === 'close') finishOld(snapshot);
                    else finishClose();
                });
                await act(async () => {
                    visibility.mockReturnValue('hidden');
                    document.dispatchEvent(new Event('visibilitychange'));
                });
                await act(async () => {
                    visibility.mockReturnValue('visible');
                    document.dispatchEvent(new Event('visibilitychange'));
                });
                expect(api.qrLoginSnapshot).toHaveBeenCalledOnce();
                expect(api.closeQrLogin).toHaveBeenCalledOnce();
                expect(result.current.recovering).toBe(true);
                expect(result.current.canRetry).toBe(false);
                expect(result.current.snapshot?.qrcode_payload).not.toBe(snapshot.qrcode_payload);
                await act(async () => {
                    if (pendingBarrier === 'close') finishClose();
                    else finishOld(snapshot);
                });
                expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(2);
                expect(api.qrLoginSnapshot).toHaveBeenLastCalledWith('123456', false);
                expect(result.current.snapshot?.qrcode_payload).toBe('resumed-code');
                expect(api.closeQrLogin).toHaveBeenCalledOnce();
            } finally {
                unmount();
                visibility.mockRestore();
            }
        },
    );

    it('keeps cleanup failure blocked when a timed-out dialog becomes visible again', async () => {
        vi.useFakeTimers();
        const visibility = vi.spyOn(document, 'visibilityState', 'get');
        let finishOld!: (value: SnowLumaQrLoginSnapshot) => void;
        let failClose!: (error: Error) => void;
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(
                () =>
                    new Promise<SnowLumaQrLoginSnapshot>((resolve) => {
                        finishOld = resolve;
                    }),
            ),
            closeQrLogin: vi.fn(
                () =>
                    new Promise<void>((_, reject) => {
                        failClose = reject;
                    }),
            ),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        try {
            await act(async () => {});
            await act(async () => vi.advanceTimersByTimeAsync(SL_QR_REQUEST_TIMEOUT_MS));
            await act(async () => {
                visibility.mockReturnValue('hidden');
                document.dispatchEvent(new Event('visibilitychange'));
            });
            await act(async () => {
                visibility.mockReturnValue('visible');
                document.dispatchEvent(new Event('visibilitychange'));
            });
            await act(async () => failClose(new Error('close failed')));
            await act(async () => finishOld(snapshot));
            expect(result.current.snapshot?.state).toBe('cleanup_failed');
            expect(result.current.canRetry).toBe(false);
            expect(api.qrLoginSnapshot).toHaveBeenCalledOnce();
            expect(api.closeQrLogin).toHaveBeenCalledOnce();
        } finally {
            unmount();
            visibility.mockRestore();
        }
    });

    it('rechecks cleanup after an explicit reopen before starting a new request', async () => {
        vi.useFakeTimers();
        let finishOld!: (value: SnowLumaQrLoginSnapshot) => void;
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockImplementationOnce(
                    () =>
                        new Promise<SnowLumaQrLoginSnapshot>((resolve) => {
                            finishOld = resolve;
                        }),
                )
                .mockResolvedValue({ ...snapshot, qrcode_payload: 'reopened-code' }),
            closeQrLogin: vi
                .fn()
                .mockRejectedValueOnce(new Error('close failed'))
                .mockResolvedValue(undefined),
        };
        const { result, rerender, unmount } = renderHook(
            ({ open }) => useRemoteSnowlumaQrLogin('123456', open, api),
            { initialProps: { open: true } },
        );
        await act(async () => {});
        await act(async () => vi.advanceTimersByTimeAsync(SL_QR_REQUEST_TIMEOUT_MS));
        expect(result.current.snapshot?.state).toBe('cleanup_failed');
        rerender({ open: false });
        await act(async () => {});
        rerender({ open: true });
        await act(async () => {});
        expect(api.closeQrLogin).toHaveBeenCalledTimes(2);
        expect(api.qrLoginSnapshot).toHaveBeenCalledOnce();
        await act(async () => finishOld(snapshot));
        expect(result.current.snapshot?.qrcode_payload).toBe('reopened-code');
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(2);
        unmount();
    });

    it('waits for every raw close request after reopening a cleanup timeout', async () => {
        vi.useFakeTimers();
        let finishOld!: (value: SnowLumaQrLoginSnapshot) => void;
        let finishOldClose!: () => void;
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi
                .fn()
                .mockImplementationOnce(
                    () =>
                        new Promise<SnowLumaQrLoginSnapshot>((resolve) => {
                            finishOld = resolve;
                        }),
                )
                .mockResolvedValue({ ...snapshot, qrcode_payload: 'reopened-code' }),
            closeQrLogin: vi
                .fn()
                .mockImplementationOnce(
                    () =>
                        new Promise<void>((resolve) => {
                            finishOldClose = resolve;
                        }),
                )
                .mockResolvedValue(undefined),
        };
        const { result, rerender, unmount } = renderHook(
            ({ open }) => useRemoteSnowlumaQrLogin('123456', open, api),
            { initialProps: { open: true } },
        );
        await act(async () => {});
        await act(async () => vi.advanceTimersByTimeAsync(SL_QR_REQUEST_TIMEOUT_MS * 2));
        expect(result.current.snapshot?.state).toBe('cleanup_failed');
        await act(async () => finishOld(snapshot));
        rerender({ open: false });
        await act(async () => {});
        rerender({ open: true });
        await act(async () => {});
        expect(api.closeQrLogin).toHaveBeenCalledTimes(2);
        expect(api.qrLoginSnapshot).toHaveBeenCalledOnce();
        expect(result.current.recovering).toBe(true);
        await act(async () => finishOldClose());
        expect(api.qrLoginSnapshot).toHaveBeenCalledTimes(2);
        expect(result.current.snapshot?.qrcode_payload).toBe('reopened-code');
        expect(api.closeQrLogin).toHaveBeenCalledTimes(2);
        unmount();
    });

    it('ends a prolonged login confirmation at the session deadline', async () => {
        vi.useFakeTimers();
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => ({ ...snapshot, state: 'login_detected' as const })),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { result, unmount } = renderHook(() => useRemoteSnowlumaQrLogin('123456', true, api));
        await act(async () => {});
        await act(async () => {
            await vi.advanceTimersByTimeAsync(SL_QR_SESSION_TIMEOUT_MS);
        });
        expect(result.current.error).toContain('已超时');
        expect(result.current.snapshot).toBeNull();
        expect(api.closeQrLogin).toHaveBeenCalledOnce();
        unmount();
    });

    it('does not cancel or duplicate the new session during StrictMode effect replay', async () => {
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => snapshot),
            closeQrLogin: vi.fn(async () => {}),
        };
        const wrapper = ({ children }: { children: ReactNode }) =>
            createElement(StrictMode, null, children);
        const { result, unmount } = renderHook(
            () => useRemoteSnowlumaQrLogin('123456', true, api, true),
            { wrapper },
        );
        await waitFor(() => expect(result.current.snapshot?.state).toBe('waiting_for_qr'));
        expect(api.qrLoginSnapshot).toHaveBeenCalledOnce();
        expect(api.closeQrLogin).not.toHaveBeenCalled();
        unmount();
    });
});
