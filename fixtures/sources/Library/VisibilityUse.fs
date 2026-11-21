module NavFixture.VisibilityUse

open NavFixture.VisibilityA
open NavFixture.VisibilityB
let selected = (*@private_shadow_use*)value

open NavFixture.SignatureApi
let signatureSelected = (*@signature_boundary_use*)hiddenBySignature
