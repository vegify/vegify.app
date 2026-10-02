import { describe, expect, it } from "vitest"

import { viewerIp } from "./api"

describe("viewerIp", () => {
  it("strips the source port CloudFront appends to the viewer address", () => {
    expect(viewerIp("198.51.100.10:46532")).toBe("198.51.100.10")
    // IPv6 is not bracketed: the port is whatever follows the last colon.
    expect(viewerIp("2001:db8::1:46532")).toBe("2001:db8::1")
  })

  it("forwards nothing when CloudFront sent no address", () => {
    expect(viewerIp(undefined)).toBeNull()
    expect(viewerIp(null)).toBeNull()
    expect(viewerIp("")).toBeNull()
  })
})
