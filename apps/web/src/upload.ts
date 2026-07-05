// Client-side image upload: resize/re-encode in the BROWSER (the t4g.nano never touches image
// bytes), then PUT to the presigned URL the server mints, and hand back the media key to attach.
import { createServerFn } from '@tanstack/react-start'
import type { UploadTicket } from '@vegify/api-types'

const uploadUrlFn = createServerFn({ method: 'POST' })
  .validator((contentType: string) => contentType)
  .handler(async ({ data }): Promise<UploadTicket> => {
    const { requestUploadUrl } = await import('./content')
    return requestUploadUrl(data)
  })

const MAX_EDGE = 1600

/** Downscale to ≤1600px on the long edge and re-encode (jpeg .85; png stays png for alpha). */
async function shrink(file: File): Promise<{ blob: Blob; contentType: string }> {
  const bitmap = await createImageBitmap(file)
  const scale = Math.min(1, MAX_EDGE / Math.max(bitmap.width, bitmap.height))
  const keepPng = file.type === 'image/png'
  const contentType = keepPng ? 'image/png' : 'image/jpeg'
  if (scale === 1 && (file.type === 'image/jpeg' || keepPng)) {
    return { blob: file, contentType: file.type }
  }
  const canvas = document.createElement('canvas')
  canvas.width = Math.round(bitmap.width * scale)
  canvas.height = Math.round(bitmap.height * scale)
  canvas.getContext('2d')!.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
  const blob = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('encode failed'))), contentType, 0.85),
  )
  return { blob, contentType }
}

/** Full client upload: shrink → presigned PUT → the media key ready to attach. */
export async function uploadImage(file: File): Promise<{ key: string; contentType: string }> {
  const { blob, contentType } = await shrink(file)
  const ticket = await uploadUrlFn({ data: contentType })
  const res = await fetch(ticket.url, {
    method: 'PUT',
    headers: { 'content-type': contentType },
    body: blob,
  })
  if (!res.ok) throw new Error(`upload failed (${res.status})`)
  return { key: ticket.key, contentType }
}
