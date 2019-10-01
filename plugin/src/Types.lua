local t = require(script.Parent.Parent.t)

local DevSettings = require(script.Parent.DevSettings)
local strict = require(script.Parent.strict)

local RbxId = t.string

local ApiValue = t.interface({
	Type = t.string,
	Value = t.optional(t.any),
})

local ApiInstanceMetadata = t.interface({
	ignoreUnknownInstances = t.optional(t.boolean),
})

local ApiInstance = t.interface({
	Name = t.string,
	ClassName = t.string,
	Properties = t.map(t.string, ApiValue),
	Metadata = t.optional(ApiInstanceMetadata),
	Children = t.array(RbxId),
})

local ApiInstanceUpdate = t.interface({
	id = RbxId,
	changedName = t.optional(t.string),
	changedClassName = t.optional(t.string),
	changedProperties = t.map(t.string, ApiValue),
	changedMetadata = t.optional(ApiInstanceMetadata),
})

local ApiSubscribeMessage = t.interface({
	removedInstances = t.array(RbxId),
	addedInstances = t.map(RbxId, ApiInstance),
	updatedInstances = t.array(ApiInstanceUpdate),
})

local ApiInfoResponse = t.interface({
	sessionId = t.string,
	serverVersion = t.string,
	protocolVersion = t.number,
	expectedPlaceIds = t.optional(t.array(t.number)),
	rootInstanceId = RbxId,
})

local ApiReadResponse = t.interface({
	sessionId = t.string,
	messageCursor = t.number,
	instances = t.map(RbxId, ApiInstance),
})

local ApiSubscribeResponse = t.interface({
	sessionId = t.string,
	messageCursor = t.number,
	messages = t.array(ApiSubscribeMessage),
})

local ApiError = t.interface({
	kind = t.union(
		t.literal("NotFound"),
		t.literal("BadRequest"),
		t.literal("InternalError")
	),
	details = t.string,
})

local function ifEnabled(innerCheck)
	return function(...)
		if DevSettings:shouldTypecheck() then
			return innerCheck(...)
		else
			return true
		end
	end
end

return strict("Types", {
	ifEnabled = ifEnabled,

	ApiInfoResponse = ApiInfoResponse,
	ApiReadResponse = ApiReadResponse,
	ApiSubscribeResponse = ApiSubscribeResponse,
	ApiError = ApiError,

	ApiInstance = ApiInstance,
	ApiInstanceMetadata = ApiInstanceMetadata,
	ApiSubscribeMessage = ApiSubscribeMessage,
	ApiValue = ApiValue,
	RbxId = RbxId,

	-- Deprecated aliases during transition
	VirtualInstance = ApiInstance,
	VirtualMetadata = ApiInstanceMetadata,
	VirtualValue = ApiValue,
})